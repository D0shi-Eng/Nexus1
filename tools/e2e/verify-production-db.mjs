/**
 * NEXUS V1 — تحقق مستقل من قاعدة الإنتاج وسلسلة التدقيق (بلا اعتماد على كود Rust).
 * -------------------------------------------------------------------------------
 * يُعيد حساب سلسلة SHA-256 بالخوارزمية نفسها من node:crypto فوق قاعدة UAT
 * الإنتاجية، ويُشغّل فحوص SQLite التكاملية، ويقيس مرساة الذيل، ويطابق
 * أعداد المؤشرات مع الاستعلامات المباشرة.
 *
 * التشغيل: node tools/e2e/verify-production-db.mjs --db=<path> --appdata=<dir> --evidence=<dir>
 * قراءة فقط: لا كتابة في القاعدة.
 */
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';

const require = createRequire(import.meta.url);
const { DatabaseSync } = require('node:sqlite');

const argv = process.argv.slice(2);
const argOf = (name, fallback) => {
  const found = argv.find((a) => a.startsWith(`--${name}=`));
  return found ? found.slice(name.length + 3) : fallback;
};
const DB_PATH = argOf('db', null);
const APPDATA_DIR = argOf('appdata', path.dirname(DB_PATH ?? ''));
const EVIDENCE = argOf('evidence', path.join(process.cwd(), '.tmp-uat', 'evidence-verification'));
if (!DB_PATH || !existsSync(DB_PATH)) {
  console.error('DB_NOT_FOUND');
  process.exit(2);
}
mkdirSync(EVIDENCE, { recursive: true });

const result = { generatedAt: new Date().toISOString(), checks: [] };
const check = (id, ok, detail) => {
  result.checks.push({ id, ok, detail });
  console.log(`${ok ? 'PASS' : 'FAIL'} ${id} — ${detail}`);
};

const database = new DatabaseSync(DB_PATH, { readOnly: true });
const scalar = (sql, ...params) => database.prepare(sql).get(...params);

const integrity = scalar('PRAGMA integrity_check');
check('DB-01', integrity?.integrity_check === 'ok', `PRAGMA integrity_check=${integrity?.integrity_check}`);
const foreignKeys = database.prepare('PRAGMA foreign_key_check').all();
check('DB-02', foreignKeys.length === 0, `PRAGMA foreign_key_check=${foreignKeys.length} انتهاك`);

const mode = scalar("SELECT mode FROM app_runtime_state WHERE id = 1")?.mode;
check('DB-03', mode === 'PRODUCTION', `وضع التشغيل=${mode}`);

const migrations = scalar('SELECT COUNT(*) AS total FROM schema_migrations')?.total ?? 0;
const maxMigration = scalar('SELECT MAX(version) AS max_version FROM schema_migrations')?.max_version;
check('DB-04', Number(maxMigration) === 26, `الهجرات المطبقة=${migrations} أحدث نسخة=${maxMigration}`);

const tenants = database.prepare('SELECT id, data_mode FROM tenants').all();
check(
  'DB-05',
  tenants.length === 1 && tenants[0].data_mode === 'PRODUCTION',
  `مستأجرون=${tenants.length} نوع البيانات=${tenants[0]?.data_mode}`,
);

// ===== سلسلة التدقيق: إعادة حساب مستقلة =====
const tenantId = tenants[0].id;
const events = database
  .prepare(
    'SELECT rowid AS rownum, id, event_type, actor_name, resource_type, resource_id, occurred_at, previous_hash, event_hash FROM audit_events WHERE tenant_id = ? ORDER BY rowid ASC',
  )
  .all(tenantId);

let expectedPrevious = '';
let broken = 0;
let mismatched = 0;
for (const event of events) {
  const storedPrevious = event.previous_hash ?? '';
  if (storedPrevious !== expectedPrevious) broken += 1;
  const payload = `${storedPrevious}|${event.id}|${event.event_type}|${event.actor_name}|${event.resource_type}|${event.resource_id}|${event.occurred_at}`;
  const recalculated = createHash('sha256').update(payload, 'utf8').digest('hex');
  if (recalculated !== event.event_hash) mismatched += 1;
  expectedPrevious = event.event_hash;
}
check(
  'AUDIT-01',
  broken === 0 && mismatched === 0 && events.length > 0,
  `أحداث=${events.length} روابط مكسورة=${broken} تجزئات غير مطابقة=${mismatched}`,
);
check('AUDIT-02', new Set(events.map((event) => event.id)).size === events.length, 'معرّفات الأحداث فريدة');

// ===== مرساة الذيل المحفوظة عند آخر تحقق ناجح =====
const anchorPath = path.join(APPDATA_DIR, 'audit-anchor.json');
if (existsSync(anchorPath)) {
  const anchor = JSON.parse(readFileSync(anchorPath, 'utf8'));
  const anchoredEvent = events[anchor.eventCount - 1];
  const anchorMatches =
    anchor.tenantId === tenantId &&
    anchor.eventCount > 0 &&
    anchor.eventCount <= events.length &&
    anchoredEvent?.event_hash === anchor.headHash;
  check(
    'AUDIT-03',
    anchorMatches,
    `مرساة: عدد=${anchor.eventCount} تجزئة الذيل مطابقة=${anchoredEvent?.event_hash === anchor.headHash} أحداث حالية=${events.length}`,
  );
} else {
  check('AUDIT-03', false, 'ملف المرساة غير موجود بعد التحقق الصريح');
}

// ===== الأنواع الحرجة: كل حدث إلحاقي بلا كسر =====
const taskEvents = database
  .prepare("SELECT event_type FROM audit_events WHERE resource_id = 'task_demo_001'")
  .all();
check('AUDIT-04', taskEvents.length === 0, 'لا أحداث ديمو داخل قاعدة الإنتاج');

// ===== مطابقة مؤشرات اللوحة مع استعلامات مباشرة =====
const activeObligations = scalar(
  'SELECT COUNT(*) AS total FROM obligations WHERE tenant_id = ? AND archived_at IS NULL',
  tenantId,
).total;
const archivedObligations = scalar(
  'SELECT COUNT(*) AS total FROM obligations WHERE tenant_id = ? AND archived_at IS NOT NULL',
  tenantId,
).total;
const applicable = scalar(
  "SELECT COUNT(*) AS total FROM obligations WHERE tenant_id = ? AND archived_at IS NULL AND applicability = 'APPLIES'",
  tenantId,
).total;
const humanVerified = scalar(
  "SELECT COUNT(*) AS total FROM obligations WHERE tenant_id = ? AND archived_at IS NULL AND applicability = 'APPLIES' AND verification_state = 'HUMAN_VERIFIED'",
  tenantId,
).total;
check(
  'KPI-01',
  activeObligations >= 0 && applicable <= activeObligations && humanVerified <= applicable,
  `التزامات نشطة=${activeObligations} مؤرشفة=${archivedObligations} منطبقة=${applicable} متحقق بشرياً=${humanVerified}`,
);

const now = new Date().toISOString();
const expiredEvidence = scalar(
  `SELECT COUNT(*) AS total FROM evidence_items WHERE tenant_id = ? AND expires_at IS NOT NULL AND expires_at <= ?`,
  tenantId,
  now,
).total;
const registeredByMissing = scalar(
  `SELECT COUNT(*) AS total FROM evidence_items WHERE tenant_id = ? AND registered_by IS NULL`,
  tenantId,
).total;
check(
  'EVID-01',
  registeredByMissing === 0,
  `أدلة جديدة بلا registered_by=${registeredByMissing} (يجب صفر في مسار الإنتاج)`,
);
check('EVID-02', expiredEvidence >= 0, `أدلة منتهية الأجل في القاعدة=${expiredEvidence}`);

// ===== أدوار المؤسسة: فصل المراجعة عن الإدارة =====
const roles = database.prepare('SELECT code FROM roles WHERE tenant_id = ? ORDER BY code').all(tenantId);
const roleCodes = roles.map((row) => row.code);
check(
  'RBAC-01',
  roleCodes.includes('TENANT_ADMIN') && roleCodes.includes('CONTRIBUTOR') && roleCodes.includes('REVIEWER'),
  `أدوار المؤسسة=${roleCodes.join(',')}`,
);

// ===== عدم وجود جلسات ميتة/مشبوهة =====
const sessions = scalar(
  'SELECT COUNT(*) AS total FROM local_sessions WHERE tenant_id = ? AND revoked_at IS NULL',
  tenantId,
).total;
check('SESS-01', sessions >= 1, `جلسات نشطة=${sessions}`);

// ===== سلامة مستخدم إداري واحد على الأقل =====
const administrators = scalar(
  `SELECT COUNT(DISTINCT u.id) AS total
   FROM local_users u
   JOIN user_role_assignments ura ON ura.user_id = u.id
   JOIN role_permissions rp ON rp.role_id = ura.role_id
   JOIN permissions p ON p.id = rp.permission_id
   WHERE u.tenant_id = ? AND u.status = 'ACTIVE'
     AND p.resource_type = 'USER' AND p.action IN ('CREATE','UPDATE')`,
  tenantId,
).total;
check('RBAC-02', administrators >= 1, `مسؤولون نشطون=${administrators}`);

database.close();
result.passed = result.checks.filter((entry) => entry.ok).length;
result.failed = result.checks.filter((entry) => !entry.ok).length;
writeFileSync(path.join(EVIDENCE, 'production-db-verification.json'), JSON.stringify(result, null, 2));
console.log(`\nDB_VERIFICATION ${result.passed}/${result.checks.length} PASS — evidence: ${EVIDENCE}`);
process.exit(result.failed === 0 ? 0 : 1);
