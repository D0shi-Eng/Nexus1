/**
 * NEXUS V1 — REAL DESKTOP UAT DRIVER
 * -----------------------------------
 * يقود نافذة WebView2 الفعلية لتطبيق Tauri الحقيقي عبر CDP، وينفّذ رحلة
 * الأعمال المؤسسية بنقرات وكتابة حقيقية على العناصر، ثم يقرأ قاعدة الاختبار
 * المعزولة مباشرةً ليؤكد أن كل أثر مكتوب فعلاً في SQLite.
 *
 * لماذا CDP: العناصر داخل WebView2 ليست في متصفح خارجي، وواجهة NEXUS تستدعي
 * أوامر Rust عبر bridge الخاص بـ Tauri. الاتصال بنافذة التطبيق الفعلية هو
 * الطريق الوحيد الذي ينفّذ الـIPC الحقيقي مع قراءة حالة React كما هي.
 *
 * الاستخدام:
 *   node nexus-uat.mjs <exePath> --phase=production|demo --evidence=<dir> [--port=9444]
 *
 * المخرجات: تقرير JSON على القرص + ملخص على stdout. لا أسرار ولا بيانات عميل.
 */
import { spawn, spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { chromium } from 'playwright-core';

const require = createRequire(import.meta.url);
const { DatabaseSync } = require('node:sqlite');

const argv = process.argv.slice(2);
const exePath = argv.find((a) => !a.startsWith('--'));
const argOf = (name, fallback) => {
  const found = argv.find((a) => a.startsWith(`--${name}=`));
  return found ? found.slice(name.length + 3) : fallback;
};
const PHASE = argOf('phase', 'demo');
const PORT = Number(argOf('port', '9444'));
const EVIDENCE = argOf('evidence', null);
const APPDATA_DIR = argOf('appdata', path.join(process.env.LOCALAPPDATA ?? '', 'com.nexus.obligationos.uatharness'));
const DB_PATH = path.join(APPDATA_DIR, 'nexus-local.db');

if (!exePath || !existsSync(exePath)) {
  console.error(`UAT_FAILURE: executable not found: ${exePath}`);
  process.exit(2);
}
if (EVIDENCE) mkdirSync(EVIDENCE, { recursive: true });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// نصوص الواجهة العربية هي الافتراضي في المنتج؛ تُستعمل كما يراها المستخدم.
const AR = {
  navTasks: 'المهام',
  navEvidence: 'الأدلة',
  navOverview: 'نظرة تنفيذية',
  navReports: 'التقارير',
  navAudit: 'التدقيق',
  navSettings: 'الإعدادات',
  navObligations: 'الالتزامات',
  navAttention: 'ما يحتاج إلى اهتمام',
  navRegulatory: 'الذكاء التنظيمي',
  navControls: 'الضوابط',
  login: 'تسجيل الدخول',
  logout: 'تسجيل الخروج',
  refresh: 'تحديث',
  open: 'فتح',
  disable: 'تعطيل',
  enable: 'تفعيل',
  decide: 'اتخاذ قرار',
  verifyObligation: 'تحقق بشري',
  settingsData: 'البيانات والأمان',
  settingsSecurity: 'الأمان وكلمة المرور',
  addMember: 'إضافة عضو',
  createAccount: 'إنشاء الحساب',
  fieldDisplayName: 'الاسم المعروض',
  fieldEmail: 'البريد الإلكتروني',
  fieldRole: 'الدور',
  fieldInitialPassword: 'كلمة مرور أولية',
  fieldConfirmPassword: 'تأكيد كلمة المرور',
  revokeSessions: 'إلغاء الجلسات',
  changePassword: 'تغيير كلمة المرور',
  fieldCurrentPassword: 'كلمة المرور الحالية',
  fieldNewPassword: 'كلمة المرور الجديدة',
  fieldConfirmPasswordNew: 'تأكيد كلمة المرور الجديدة',
  registerSource: 'تسجيل مصدر تنظيمي',
  saveSource: 'تسجيل المصدر',
  fieldAuthority: 'الجهة المنسوب إليها المصدر',
  fieldDocumentTitle: 'عنوان الوثيقة',
  fieldDocumentVersion: 'الإصدار',
  fieldSourceUrl: 'الرابط المرجعي (اختياري)',
  createObligation: 'إنشاء التزام',
  saveObligation: 'إنشاء الالتزام',
  fieldObligationTitle: 'عنوان الالتزام',
  fieldObligationDescription: 'الوصف',
  fieldObligationOwner: 'المسؤول عن التنفيذ',
  fieldDueDate: 'الموعد',
  fieldPriority: 'الأولوية',
  fieldApplicability: 'الانطباق',
  startDemo: 'بدء التجربة التجريبية',
  submitBootstrap: 'تهيئة مساحة الإنتاج',
  registerEvidence: 'تسجيل دليل مرجعي',
  registerEvidencePage: 'تسجيل مرجع دليل',
  createTask: 'إنشاء مهمة',
  applyAction: 'تنفيذ الإجراء',
  assignConfirm: 'إسناد',
  history: 'السجل',
  review: 'مراجعة',
  approve: 'اعتماد',
  reject: 'رفض',
  transitionStart: 'بدء العمل',
  transitionSubmit: 'طلب المراجعة',
  transitionClose: 'إغلاق المهمة',
  statusOpen: 'مفتوح',
  statusInProgress: 'قيد التنفيذ',
  statusReady: 'بانتظار المراجعة',
  statusApproved: 'معتمدة',
  statusRejected: 'مرفوض',
  statusCompleted: 'مكتمل',
  fieldTaskTitle: 'عنوان المهمة',
  fieldDescription: 'وصف الإجراء',
  fieldEvidenceLabel: 'وصف الدليل',
  fieldFingerprint: 'بصمة المحتوى أو المرجع',
  fieldReason: 'سبب القرار',
  fieldActionReason: 'سبب الإجراء (اختياري، ويُحفظ في سجل الانتقالات)',
  exportCsv: 'تصدير CSV',
  exportMarkdown: 'تصدير Markdown',
  readinessNotMeasured: 'غير مقيس',
  chainValid: 'سلسلة التدقيق سليمة',
  chainInvalid: 'سلسلة التدقيق غير سليمة',
};

const TASK_TITLE = 'Prepare synthetic data inventory evidence';
const OBLIGATION_TITLE = 'Review the synthetic data inventory evidence';

const report = {
  harness: 'nexus-uat-real-desktop',
  phase: PHASE,
  startedAt: new Date().toISOString(),
  exe: exePath,
  isolatedAppData: APPDATA_DIR,
  databasePath: DB_PATH,
  consoleErrors: [],
  pageErrors: [],
  steps: [],
  db: {},
  summary: { total: 0, passed: 0, failed: 0, skipped: 0 },
  limitations: [],
};

const step = async (name, fn, opts = {}) => {
  report.summary.total += 1;
  const entry = { name, status: 'FAIL', detail: null, durationMs: 0 };
  const t0 = Date.now();
  try {
    const detail = await fn();
    entry.status = 'PASS';
    entry.detail = detail ?? null;
  } catch (error) {
    if (opts.optional) {
      entry.status = 'SKIP';
      entry.detail = String(error?.message ?? error).slice(0, 400);
    } else {
      entry.detail = String(error?.message ?? error).slice(0, 600);
    }
  }
  entry.durationMs = Date.now() - t0;
  if (entry.status === 'PASS') report.summary.passed += 1;
  else if (entry.status === 'SKIP') report.summary.skipped += 1;
  else report.summary.failed += 1;
  report.steps.push(entry);
  console.log(`[${entry.status}] ${name} (${entry.durationMs}ms)${entry.status === 'PASS' ? '' : ' :: ' + entry.detail}`);
  return entry;
};

const assert = (condition, message) => {
  if (!condition) throw new Error(message);
};

const waitForCdp = async (port, timeoutMs = 90000) => {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`http://127.0.0.1:${port}/json/version`);
      if (res.ok) return await res.json();
    } catch {
      /* المنفذ لم يُفتح بعد */
    }
    await sleep(400);
  }
  throw new Error(`CDP endpoint on port ${port} did not open within ${timeoutMs}ms`);
};

// ===== وصول آمن للقراءة فقط إلى قاعدة الاختبار المعزولة =====
const withDb = (fn) => {
  const db = new DatabaseSync(DB_PATH, { readOnly: true });
  try {
    return fn(db);
  } finally {
    db.close();
  }
};
const scalar = (sql, ...params) => withDb((db) => {
  const row = db.prepare(sql).get(...params);
  return row ? Object.values(row)[0] : undefined;
});
const rows = (sql, ...params) => withDb((db) => db.prepare(sql).all(...params));

const auditChainCheck = () => withDb((db) => {
  const list = db.prepare('SELECT rowid AS rid, previous_hash, event_hash FROM audit_events ORDER BY rowid ASC').all();
  let broken = 0;
  let previous = '';
  for (const event of list) {
    if (event.previous_hash !== previous) broken += 1;
    previous = event.event_hash;
  }
  return { events: list.length, brokenLinks: broken };
});

// مشغّل النافذة: يمكن إعادة تشغيل التطبيق فعلياً في منتصف الجولة (اختبار الاستمرارية
// وإعادة الدخول) بدل الاكتفاء بافتراض أن البيانات باقية.
let child = null;
const launchApp = () => {
  child = spawn(exePath, [], {
    env: {
      ...process.env,
      // المنفذ مضمّن أيضاً في بيان البناء؛ المتغيّر هنا احتياط صريح.
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT} --remote-allow-origins=http://127.0.0.1:${PORT}`,
    },
    stdio: 'ignore',
    detached: false,
  });
};
launchApp();

let browser = null;
let page = null;

const ui = {
  // أي حوار مفتوح يحجب النقر على التنقل — يُغلق أولاً بأمان قبل الانتقال.
  async closeAnyDialog() {
    for (let attempt = 0; attempt < 3; attempt += 1) {
      const count = await page.locator('[role="dialog"]').count();
      if (count === 0) return;
      await page.keyboard.press('Escape');
      await sleep(250);
    }
  },
  async nav(name) {
    await ui.closeAnyDialog();
    const item = page.locator('nav.navigation button', { hasText: name }).first();
    await item.click({ timeout: 20000 });
    await sleep(500);
  },
  async button(name, root = page) {
    return root.getByRole('button', { name, exact: true }).first();
  },
  async clickButton(name, root = page) {
    const button = root.getByRole('button', { name, exact: true }).first();
    await button.waitFor({ state: 'visible', timeout: 15000 });
    await button.click();
    await sleep(350);
  },
  async heading() {
    return (await page.locator('main h1').first().textContent()) ?? '';
  },
  async contentLength() {
    return page.evaluate(() => (document.querySelector('main')?.textContent ?? '').trim().length);
  },
  async formError() {
    const errors = await page.locator('.form-error').allTextContents();
    return errors.join(' | ').trim();
  },
  async feedback() {
    const items = await page.locator('.operation-feedback').allTextContents();
    return items.join(' | ').trim();
  },
  async taskRow() {
    return page.locator('#tasks table tbody tr', { hasText: TASK_TITLE }).first();
  },
  async rowStatus() {
    const row = await ui.taskRow();
    return (await row.textContent()) ?? '';
  },
  async waitRowStatus(label, timeout = 15000) {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      const text = await ui.rowStatus().catch(() => '');
      if (text.includes(label)) return text;
      await sleep(250);
    }
    throw new Error(`task row never showed status "${label}" — last text: ${(await ui.rowStatus().catch(() => '')).slice(0, 300)}`);
  },
  async dialog() {
    return page.locator('[role="dialog"]').first();
  },
  async waitDialog() {
    const dialog = page.locator('[role="dialog"]').first();
    await dialog.waitFor({ state: 'visible', timeout: 15000 });
    return dialog;
  },
  async waitDialogClosed(timeout = 15000) {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      const count = await page.locator('[role="dialog"]').count();
      if (count === 0) return;
      await sleep(200);
    }
    throw new Error('dialog did not close');
  },
  async applyInDialog() {
    const dialog = await ui.dialog();
    await ui.clickButton(AR.applyAction, dialog);
    await sleep(500);
    await ui.waitDialogClosed();
  },
  // تنفيذ إجراء يُتوقّع نجاحه: نجاح = إغلاق الحوار، وفشل = رسالة خطأ ظاهرة.
  async applyExpectSuccess() {
    const dialog = await ui.dialog();
    await ui.clickButton(AR.applyAction, dialog);
    const deadline = Date.now() + 15000;
    while (Date.now() < deadline) {
      if ((await page.locator('[role="dialog"]').count()) === 0) return;
      const error = await ui.formError();
      if (error) throw new Error(`action rejected by backend: ${error}`);
      await sleep(200);
    }
    throw new Error('dialog did not close after action');
  },
  async ipc(command, args) {
    return page.evaluate(
      async ([cmd, payload]) => {
        try {
          const result = await window.__TAURI_INTERNALS__.invoke(cmd, payload);
          return { ok: true, result };
        } catch (error) {
          if (error && typeof error === 'object' && typeof error.code === 'string') {
            return { ok: false, code: error.code, message: error.message };
          }
          return { ok: false, code: 'UNKNOWN', message: String(error) };
        }
      },
      [command, args ?? {}],
    );
  },
  async selectActor(userId) {
    const select = page.locator('.actor-switcher select').first();
    await select.waitFor({ state: 'visible', timeout: 15000 });
    await select.selectOption(userId);
    await sleep(900);
  },
};

const shot = async (name) => {
  if (!EVIDENCE) return;
  await page.screenshot({ path: path.join(EVIDENCE, `${name}.png`), fullPage: false }).catch(() => {});
};

const saveReport = (suffix = PHASE) => {
  if (!EVIDENCE) return;
  writeFileSync(path.join(EVIDENCE, `uat-${suffix}-report.json`), JSON.stringify(report, null, 2), 'utf8');
};

// =====================================================================
// المرحلة الإنتاجية الكاملة (UAT 18 أكتوبر — جولة إغلاق العيوب)
// ---------------------------------------------------------------------
// رحلة مؤسسة حقيقية من شاشة التأسيس إلى التقرير النهائي في نافذة WebView2
// الفعلية: لا بذر ديمو، ولا كتابة مباشرة إلى SQL، ولا Mock IPC.
//   A التأسيس وإعادة التشغيل وإعادة الدخول
//   B الموظف والمراجع وكلمة المرور والجلسات
//   C المصدر التنظيمي والالتزام + الاستمرارية بعد إعادة التشغيل
//   D المهمة والأدلة والرفض وإعادة العمل والاعتماد المستقل
//   E اللوحات والتقارير والتصدير والتدقيق والسلبيات
// =====================================================================
const runProductionPhase = async () => {
  const ADMIN = {
    organization: 'UAT Synthetic Organization',
    name: 'UAT Administrator',
    email: 'uat.admin@example.test',
    password: 'Uat!Str0ng#Passw0rd26',
    country: 'QA',
    industry: 'Financial Services',
  };
  const EMPLOYEE = {
    name: 'UAT Field Operator',
    email: 'uat.operator@example.test',
    password: 'Operat0r!Passw0rd26',
    rotatedPassword: 'Operat0r!Rotated#2026',
  };
  const REVIEWER = {
    name: 'UAT Independent Reviewer',
    email: 'uat.reviewer@example.test',
    password: 'Review3r!Passw0rd#26',
  };
  const SOURCE = {
    authority: 'UAT Data Protection Authority (user-provided)',
    document: 'UAT Personal Data Protection Regulation',
    version: '1.0',
    url: 'https://example.invalid/uat/regulation-1',
  };
  const OBLIGATION_TITLE = 'UAT: review the data inventory evidence';
  const TASK_BODY = 'UAT: prepare the data inventory evidence';
  const REJECT_REASON = 'UAT: الدليل المرجعي غير كافٍ — أعد ربط دليل مطابق.';
  const APPROVE_REASON = 'UAT: الدليل مطابق والتحقق مكتمل — اعتماد.';
  const userIdOf = (email) => scalar('SELECT id FROM local_users WHERE email = ?', email);
  const taskId = () => scalar('SELECT id FROM tasks WHERE title = ?', TASK_BODY);
  const evidenceIds = () => rows('SELECT id, status, version, registered_by, obligation_id FROM evidence_items ORDER BY rowid ASC');
  const pendingApproval = () => withDb((db) => db.prepare("SELECT id, resource_version, requested_by, decision FROM approvals WHERE resource_type='TASK' AND decision='PENDING' ORDER BY rowid DESC LIMIT 1").get());

  // ---------- A — التأسيس وإعادة التشغيل وإعادة الدخول ----------
  await step('A1 — شاشة التأسيس تظهر على قاعدة معزولة نظيفة', async () => {
    await page.waitForSelector('#setup-title', { timeout: 90000 });
    const choices = await page.locator('#setup-country-code option').count();
    const initialCountry = await page.inputValue('#setup-country-code');
    assert(choices > 200, `country catalogue too small: ${choices}`);
    assert(initialCountry === '', `country must be unselected by default, got "${initialCountry}"`);
    await shot('prod-01-setup');
    return { countryOptions: choices, initialCountry, mode: 'UNPROVISIONED' };
  });

  await step('A2 — تأسيس المؤسسة بدولة فعلية وقطاع يكتبه المستخدم', async () => {
    await page.fill('#setup-organization-name', ADMIN.organization);
    await page.fill('#setup-admin-display-name', ADMIN.name);
    await page.selectOption('#setup-country-code', ADMIN.country);
    await page.fill('#setup-industry', ADMIN.industry);
    await page.fill('#setup-admin-email', ADMIN.email);
    await page.fill('#setup-password', ADMIN.password);
    await page.getByRole('button', { name: AR.submitBootstrap, exact: true }).first().click();
    await page.waitForSelector('nav.navigation', { timeout: 90000 });
    await shot('prod-02-shell');
    return { country: ADMIN.country, industry: ADMIN.industry };
  });

  await step('A3 — القاعدة مؤسسة واحدة ومسؤول واحد وبلا أي سجل أعمال مبذور', async () => {
    const snapshot = {
      tenants: scalar('SELECT COUNT(*) FROM tenants'),
      organizations: scalar('SELECT COUNT(*) FROM organizations'),
      orgRow: rows('SELECT legal_name, country_code, industry FROM organizations'),
      users: scalar('SELECT COUNT(*) FROM local_users'),
      roles: rows('SELECT code FROM roles ORDER BY code'),
      sessions: scalar('SELECT COUNT(*) FROM local_sessions WHERE revoked_at IS NULL'),
      runtimeMode: scalar('SELECT mode FROM app_runtime_state WHERE id = 1'),
      obligations: scalar('SELECT COUNT(*) FROM obligations'),
      tasks: scalar('SELECT COUNT(*) FROM tasks'),
      evidence: scalar('SELECT COUNT(*) FROM evidence_items'),
      approvals: scalar('SELECT COUNT(*) FROM approvals'),
      risks: scalar('SELECT COUNT(*) FROM risks'),
      controls: scalar('SELECT COUNT(*) FROM controls'),
      schemaVersion: scalar('SELECT MAX(version) FROM schema_migrations'),
    };
    report.db.production = snapshot;
    assert(snapshot.tenants === 1, `expected 1 tenant, got ${snapshot.tenants}`);
    assert(snapshot.organizations === 1, `expected 1 organization, got ${snapshot.organizations}`);
    assert(snapshot.users === 1, `expected exactly the first admin, got ${snapshot.users}`);
    assert(snapshot.runtimeMode === 'PRODUCTION', `runtime mode must be PRODUCTION, got ${snapshot.runtimeMode}`);
    const org = snapshot.orgRow[0];
    assert(org.country_code === ADMIN.country && org.industry === ADMIN.industry, `stored organization profile mismatch: ${JSON.stringify(org)}`);
    for (const key of ['obligations', 'tasks', 'evidence', 'approvals', 'risks', 'controls']) {
      assert(snapshot[key] === 0, `production bootstrap must not seed business records: ${key}=${snapshot[key]}`);
    }
    assert(snapshot.schemaVersion === 26, `schema version expected 26, got ${snapshot.schemaVersion}`);
    assert(snapshot.roles.some((role) => role.code === 'TENANT_ADMIN'), 'TENANT_ADMIN role missing after bootstrap');
    assert(snapshot.roles.some((role) => role.code === 'CONTRIBUTOR'), 'CONTRIBUTOR role missing after bootstrap');
    return snapshot;
  });

  await step('A4 — سلسلة التدقيق سليمة بعد التأسيس', async () => {
    const chain = auditChainCheck();
    report.db.productionAuditChain = chain;
    assert(chain.events > 0, 'no audit events recorded');
    assert(chain.brokenLinks === 0, `audit chain has ${chain.brokenLinks} broken links`);
    return chain;
  });

  await step('A5 — إغلاق التطبيق وإعادة تشغيله فعلياً', async () => {
    await restartApp();
    await shot('prod-03-restarted');
    const onLogin = await page.locator('#login-email').count();
    assert(onLogin === 1, 'a restarted production app must require signing in again');
    return { restarted: true, requiresLogin: true };
  });

  await step('A6 — الدخول بكلمة مرور المسؤول الصحيحة بعد إعادة التشغيل', async () => {
    await uiJourney.login(ADMIN.email, ADMIN.password);
    await shot('prod-04-relogin');
    return { authenticated: true };
  });

  await step('A7 — سلسلة التدقيق تمتد بعد الدخول بلا كسر', async () => {
    const chain = auditChainCheck();
    const authEvents = scalar("SELECT COUNT(*) FROM audit_events WHERE event_type = 'USER_AUTHENTICATED'");
    assert(chain.brokenLinks === 0, `audit chain broken after login: ${chain.brokenLinks}`);
    assert(authEvents >= 1, 'USER_AUTHENTICATED event missing');
    return { ...chain, authEvents };
  });

  // ---------- B — الموظفون وكلمة المرور والجلسات ----------
  await step('B1 — إنشاء الموظف والمراجع المستقل من الواجهة (UAT-01)', async () => {
    await uiJourney.settingsSection(AR.settingsData);
    const create = page.getByRole('button', { name: AR.addMember, exact: true }).first();
    await create.waitFor({ state: 'visible', timeout: 30000 });

    const openAndCreate = async (member, roleCode) => {
      await page.getByRole('button', { name: AR.addMember, exact: true }).first().click();
      const dialog = await ui.waitDialog();
      await uiJourney.fillByLabel(dialog, AR.fieldDisplayName, member.name);
      await uiJourney.fillByLabel(dialog, AR.fieldEmail, member.email);
      await dialog.locator('select').first().selectOption(roleCode);
      await uiJourney.fillByLabel(dialog, AR.fieldInitialPassword, member.password);
      await uiJourney.fillByLabel(dialog, AR.fieldConfirmPassword, member.password);
      await uiJourney.click(dialog, AR.createAccount);
      await ui.waitDialogClosed(20000);
      await sleep(800);
    };

    await openAndCreate(EMPLOYEE, 'CONTRIBUTOR');
    await openAndCreate(REVIEWER, 'TENANT_ADMIN');
    const employeeId = userIdOf(EMPLOYEE.email);
    const reviewerId = userIdOf(REVIEWER.email);
    assert(employeeId, 'employee account row missing');
    assert(reviewerId, 'reviewer account row missing');
    await shot('prod-05-users');
    return { employeeId, reviewerId };
  });

  await step('B2 — الصلاحيات الفعلية والتدقيق: الموظف بلا سلطة إدارة والمراجع أهلية قرار', async () => {
    const employeeId = userIdOf(EMPLOYEE.email);
    const reviewerId = userIdOf(REVIEWER.email);
    const grantsOf = (userId) => rows(
      "SELECT p.code FROM user_role_assignments ura INNER JOIN role_permissions rp ON rp.role_id = ura.role_id INNER JOIN permissions p ON p.id = rp.permission_id WHERE ura.user_id = ? ORDER BY p.code",
      userId,
    ).map((row) => row.code);
    const employeeGrants = grantsOf(employeeId);
    const reviewerGrants = grantsOf(reviewerId);
    const provisioned = scalar("SELECT COUNT(*) FROM audit_events WHERE event_type = 'USER_PROVISIONED'");
    report.db.userProvisioning = { employeeGrants, reviewerGrants, provisioned };
    assert(employeeGrants.includes('task:update'), 'contributor must be able to move their own task');
    assert(employeeGrants.includes('evidence:create'), 'contributor must be able to register evidence');
    assert(!employeeGrants.includes('user:create'), 'contributor must not administer users');
    assert(!employeeGrants.includes('evidence:update'), 'contributor must not decide evidence verification (UAT-04)');
    assert(!employeeGrants.includes('approval:decide'), 'contributor must not approve work');
    assert(reviewerGrants.includes('approval:decide'), 'reviewer must hold the approval decision role');
    assert(reviewerGrants.includes('evidence:update'), 'reviewer decides evidence verification state');
    assert(provisioned === 2, `expected 2 USER_PROVISIONED events, got ${provisioned}`);
    return { employeeGrants, reviewerGrants, provisioned };
  });

  await step('B3 — خروج المسؤول: إنشاء مستخدم بلا هوية مرفوض خلفياً بلا صف', async () => {
    await uiJourney.logout();
    const before = scalar('SELECT COUNT(*) FROM local_users');
    const attempt = await ui.ipc('create_workspace_user', {
      input: {
        displayName: 'Unauthenticated Ghost',
        email: 'ghost@example.test',
        initialPassword: 'Gh0st!Passw0rd#2026',
        roleCodes: ['CONTRIBUTOR'],
      },
    });
    const after = scalar('SELECT COUNT(*) FROM local_users');
    assert(attempt.ok === false, 'unauthenticated user creation unexpectedly succeeded');
    assert(attempt.code === 'AUTHORIZATION_DENIED', `wrong rejection code: ${JSON.stringify(attempt)}`);
    assert(before === after, 'unauthenticated creation must not write rows');
    return { attempt, rowsUnchanged: true };
  });

  await step('B4 — الموظف يدخل ويرى صلاحياته الفعلية بلا زر إدارة مستخدمين', async () => {
    await uiJourney.login(EMPLOYEE.email, EMPLOYEE.password);
    const navCount = await page.locator('nav.navigation button').count();
    assert(navCount === 15, `expected 15 modules for the employee, got ${navCount}`);
    await ui.nav(AR.navSettings);
    const dataTab = await page.locator('.settings-nav__item', { hasText: AR.settingsData }).count();
    const addMemberButtons = await page.getByRole('button', { name: AR.addMember, exact: true }).count();
    assert(dataTab === 0, 'employee must not see the user-data settings section (no user:read)');
    assert(addMemberButtons === 0, 'employee must not see the add-member action');
    await ui.nav(AR.navTasks);
    await shot('prod-06-employee-shell');
    return { navCount, addMemberButtons, dataTabVisible: dataTab > 0 };
  });

  await step('B5 — الموظف يحاول إنشاء حساب: رفض خلفي حتى لو تجاوز الواجهة', async () => {
    const before = scalar('SELECT COUNT(*) FROM local_users');
    const attempt = await ui.ipc('create_workspace_user', {
      input: {
        displayName: 'Employee Attempt',
        email: 'employee.attempt@example.test',
        initialPassword: 'Str0ng!Passw0rd#2026',
        roleCodes: ['TENANT_ADMIN'],
      },
    });
    const after = scalar('SELECT COUNT(*) FROM local_users');
    assert(attempt.ok === false, 'privilege escalation through IPC unexpectedly succeeded');
    assert(attempt.code === 'AUTHORIZATION_DENIED', `wrong escalation rejection: ${JSON.stringify(attempt)}`);
    assert(before === after, 'rejected escalation must not write rows');
    return { attempt, rowsUnchanged: true };
  });

  await step('B6 — الموظف يغيّر كلمة مروره فتُبطل جلسته السابقة (UAT-02)', async () => {
    const second = await ui.ipc('login', { input: { email: EMPLOYEE.email, password: EMPLOYEE.password } });
    assert(second.ok === true, `second login failed: ${JSON.stringify(second)}`);
    const activeBefore = scalar("SELECT COUNT(*) FROM local_sessions s INNER JOIN local_users u ON u.id = s.user_id WHERE u.email = ? AND s.revoked_at IS NULL", EMPLOYEE.email);
    assert(activeBefore >= 2, `expected at least 2 active employee sessions, got ${activeBefore}`);

    await uiJourney.settingsSection(AR.settingsSecurity);
    await uiJourney.fillByLabel(page, AR.fieldCurrentPassword, EMPLOYEE.password);
    await uiJourney.fillByLabel(page, AR.fieldNewPassword, EMPLOYEE.rotatedPassword);
    await uiJourney.fillByLabel(page, AR.fieldConfirmPasswordNew, EMPLOYEE.rotatedPassword);
    await uiJourney.click(page, AR.changePassword);
    await page.waitForSelector('.system-note', { timeout: 20000 });
    const notice = (await page.locator('.system-note').first().textContent()) ?? '';
    assert(notice.includes('تم تغيير كلمة المرور') || notice.length > 0, 'password change confirmation missing');
    await shot('prod-07-password-changed');

    const revoked = scalar("SELECT COUNT(*) FROM local_sessions s INNER JOIN local_users u ON u.id = s.user_id WHERE u.email = ? AND s.revoked_at IS NOT NULL", EMPLOYEE.email);
    const changedEvents = scalar("SELECT COUNT(*) FROM audit_events WHERE event_type = 'USER_PASSWORD_CHANGED'");
    const storedHash = scalar('SELECT password_hash FROM local_users WHERE email = ?', EMPLOYEE.email);
    report.db.password = { notice: notice.trim(), activeBefore, revoked, changedEvents };
    assert(revoked >= 1, 'changing the password must revoke the previous sessions');
    assert(changedEvents === 1, `expected one USER_PASSWORD_CHANGED event, got ${changedEvents}`);
    assert(typeof storedHash === 'string' && storedHash.startsWith('$argon2id$'), 'stored password must remain an Argon2id PHC hash');
    assert(!String(storedHash).includes(EMPLOYEE.rotatedPassword), 'the password must never be stored in clear text');
    return report.db.password;
  });

  await step('B7 — كلمة المرور القديمة مرفوضة والجديدة مقبولة', async () => {
    await uiJourney.logout();
    await page.fill('#login-email', EMPLOYEE.email);
    await page.fill('#login-password', EMPLOYEE.password);
    await page.getByRole('button', { name: AR.login, exact: true }).first().click();
    await sleep(1800);
    const errorText = (await page.locator('.form-error, .error-panel').allTextContents()).join(' | ');
    assert(errorText.length > 0, 'the old password must be rejected, but no error was shown');
    assert(await page.locator('#login-email').count() === 1, 'rejected login must stay on the login screen');
    await uiJourney.login(EMPLOYEE.email, EMPLOYEE.rotatedPassword);
    await shot('prod-08-new-password');
    return { oldPasswordRejected: true, errorText: errorText.slice(0, 200) };
  });

  await step('B8 — تغيير كلمة المرور بلا الكلمة الحالية مرفوض خلفياً', async () => {
    const before = scalar('SELECT password_hash FROM local_users WHERE email = ?', EMPLOYEE.email);
    const attempt = await ui.ipc('change_password', {
      input: { currentPassword: 'Wr0ng!Passw0rd#2026', newPassword: 'An0ther!Passw0rd#2026' },
    });
    const after = scalar('SELECT password_hash FROM local_users WHERE email = ?', EMPLOYEE.email);
    assert(attempt.ok === false, 'password change with a wrong current password must be rejected');
    assert(attempt.code === 'CURRENT_PASSWORD_INVALID', `wrong rejection code: ${JSON.stringify(attempt)}`);
    assert(before === after, 'rejected password change must not modify the stored hash');
    return { attempt, hashUnchanged: true };
  });

  await step('B9 — تعطيل حساب الموظف يقطع وصوله فعلياً ثم يُعاد تفعيله', async () => {
    await uiJourney.logout();
    await uiJourney.login(ADMIN.email, ADMIN.password);
    await uiJourney.settingsSection(AR.settingsData);
    const employeeId = userIdOf(EMPLOYEE.email);
    const row = page.locator('table tbody tr', { hasText: EMPLOYEE.name }).first();
    await row.waitFor({ state: 'visible', timeout: 20000 });
    await row.getByRole('button', { name: AR.revokeSessions, exact: true }).first().click();
    const revokeConfirm = await ui.waitDialog();
    await uiJourney.click(revokeConfirm, AR.revokeSessions);
    await ui.waitDialogClosed(20000);
    await sleep(1200);
    const sessionsAfterRevoke = scalar("SELECT COUNT(*) FROM local_sessions s INNER JOIN local_users u ON u.id = s.user_id WHERE u.email = ? AND s.revoked_at IS NULL", EMPLOYEE.email);
    assert(sessionsAfterRevoke === 0, `expected all employee sessions revoked, got ${sessionsAfterRevoke}`);

    const disableRow = page.locator('table tbody tr', { hasText: EMPLOYEE.name }).first();
    await disableRow.getByRole('button', { name: AR.disable, exact: true }).first().click();
    const disableConfirm = await ui.waitDialog();
    await uiJourney.click(disableConfirm, AR.disable);
    await ui.waitDialogClosed(20000);
    await sleep(1200);
    const employeeStatus = scalar('SELECT status FROM local_users WHERE id = ?', employeeId);
    assert(employeeStatus === 'DISABLED', `employee status should be DISABLED, got ${employeeStatus}`);

    await uiJourney.logout();
    await page.fill('#login-email', EMPLOYEE.email);
    await page.fill('#login-password', EMPLOYEE.rotatedPassword);
    await page.getByRole('button', { name: AR.login, exact: true }).first().click();
    await sleep(1800);
    assert(await page.locator('#login-email').count() === 1, 'a disabled user must not be able to sign in');
    await shot('prod-09-disabled-login');

    await uiJourney.login(ADMIN.email, ADMIN.password);
    await uiJourney.settingsSection(AR.settingsData);
    const enableRow = page.locator('table tbody tr', { hasText: EMPLOYEE.name }).first();
    await enableRow.getByRole('button', { name: AR.enable, exact: true }).first().click();
    await sleep(1500);
    assert(scalar('SELECT status FROM local_users WHERE id = ?', employeeId) === 'ACTIVE', 'employee must be re-enabled');
    return { sessionsAfterRevoke, disabledLoginRejected: true, reEnabled: true };
  });

  await step('B10 — إلغاء الجلسات من لوحة الإدارة مرفوض للذات ولغير الأعضاء', async () => {
    const selfAttempt = await ui.ipc('revoke_user_sessions', { input: { userId: scalar("SELECT id FROM local_users WHERE email = ?", ADMIN.email) } });
    assert(selfAttempt.ok === false, 'self session revocation must be refused');
    assert(selfAttempt.code === 'SELF_MODIFICATION_FORBIDDEN', `wrong self rejection: ${JSON.stringify(selfAttempt)}`);
    const ghostAttempt = await ui.ipc('revoke_user_sessions', { input: { userId: 'user_from_another_tenant' } });
    assert(ghostAttempt.ok === false, 'cross-tenant session revocation must be refused');
    assert(ghostAttempt.code === 'RESOURCE_NOT_FOUND', `wrong cross-tenant rejection: ${JSON.stringify(ghostAttempt)}`);
    return { selfAttempt, ghostAttempt };
  });

  // ---------- C — المصدر التنظيمي والالتزام ----------
  await step('C1 — تسجيل مصدر تنظيمي يقدّمه المستخدم من الواجهة (UAT-03)', async () => {
    await ui.nav(AR.navRegulatory);
    await page.getByRole('button', { name: AR.registerSource, exact: true }).first().click();
    const dialog = await ui.waitDialog();
    await uiJourney.fillByLabel(dialog, AR.fieldAuthority, SOURCE.authority);
    await uiJourney.fillByLabel(dialog, AR.fieldDocumentTitle, SOURCE.document);
    await uiJourney.fillByLabel(dialog, AR.fieldDocumentVersion, SOURCE.version);
    await uiJourney.fillByLabel(dialog, AR.fieldSourceUrl, SOURCE.url);
    await uiJourney.click(dialog, AR.saveSource);
    await ui.waitDialogClosed(20000);
    await sleep(1200);

    const source = withDb((db) => db.prepare('SELECT id, official, verification_state, source_url, document_version FROM regulatory_sources').get());
    assert(source, 'the regulatory source row was not created');
    assert(source.official === 0, 'a user-provided source must not be stored as official');
    assert(source.verification_state === 'REQUIRES_REVIEW', `source must require review, got ${source.verification_state}`);
    assert(source.source_url === SOURCE.url && source.document_version === SOURCE.version, 'source fields were not stored literally');
    const body = await page.evaluate(() => document.querySelector('main')?.innerText ?? '');
    assert(body.includes(SOURCE.authority) && body.includes(SOURCE.document), 'the registered source is not visible in the registry');
    assert(body.includes('مستخدمو مؤسستك') || body.includes('USER_PROVIDED'), 'the page must declare user-provided provenance');
    await shot('prod-10-source');
    report.db.regulatorySource = source;
    return source;
  });

  await step('C2 — إنشاء التزام مؤسسي مرتبط بالمصدر من الواجهة (UAT-03)', async () => {
    await ui.nav(AR.navObligations);
    await page.getByRole('button', { name: AR.createObligation, exact: true }).first().click();
    const dialog = await ui.waitDialog();
    await uiJourney.fillByLabel(dialog, AR.fieldObligationTitle, OBLIGATION_TITLE);
    await uiJourney.fillByLabel(dialog, AR.fieldObligationDescription, 'التزام مؤسسي حقيقي يُنشأ من واجهة الإنتاج للتحقق من رحلة القبول.');
    await uiJourney.fillByLabel(dialog, AR.fieldObligationOwner, 'Privacy Officer');
    await uiJourney.fillByLabel(dialog, AR.fieldDueDate, '2030-06-30');
    await dialog.locator('select').nth(1).selectOption('CRITICAL');
    await dialog.locator('select').nth(2).selectOption('APPLIES');
    await uiJourney.click(dialog, AR.saveObligation);
    // بعد الإنشاء يفتح التطبيق تفاصيل السجل المنشأ مباشرةً — انتظارها هو إثبات النجاح.
    await page.waitForSelector('#obligation-details-title', { timeout: 25000 });
    await sleep(800);

    const obligation = withDb((db) => db.prepare('SELECT id, source_id, source_type, verification_state, status, owner, deadline, priority, applicability FROM obligations').get());
    assert(obligation, 'the obligation row was not created');
    assert(obligation.source_type === 'USER_PROVIDED', `provenance must be USER_PROVIDED, got ${obligation.source_type}`);
    assert(obligation.verification_state === 'UNVERIFIED', `verification must start UNVERIFIED, got ${obligation.verification_state}`);
    assert(obligation.status === 'OPEN', `new obligation must start OPEN, got ${obligation.status}`);
    assert(obligation.source_id === report.db.regulatorySource.id, 'obligation is not linked to the registered source');
    const body = await page.evaluate(() => document.querySelector('main')?.innerText ?? '');
    assert(body.includes(OBLIGATION_TITLE), 'the created obligation is not listed');
    await shot('prod-11-obligation');
    await page.keyboard.press('Escape');
    await ui.waitDialogClosed(20000);
    report.db.obligation = obligation;
    return obligation;
  });

  await step('C3 — رفض رفع المنشأ إلى رسمي/متحقق عبر حقول مصطنعة', async () => {
    const attempt = await ui.ipc('create_regulatory_source', {
      input: {
        authority: 'Fake Official Authority',
        documentTitle: 'Fake Official Document',
        documentVersion: '9.9',
        sourceUrl: 'https://example.invalid/uat/fake',
        official: true,
        verificationState: 'HUMAN_VERIFIED',
        tenantId: 'tenant_forged',
      },
    });
    assert(attempt.ok === true, `the source call should succeed while ignoring forged fields: ${JSON.stringify(attempt)}`);
    const forged = withDb((db) => db.prepare("SELECT official, verification_state, tenant_id FROM regulatory_sources WHERE authority = 'Fake Official Authority'").get());
    assert(forged, 'forged source row missing');
    assert(forged.official === 0, 'forged "official" flag must be ignored by the backend');
    assert(forged.verification_state === 'REQUIRES_REVIEW', 'forged verification state must be ignored');
    assert(forged.tenant_id === scalar('SELECT id FROM tenants'), 'the source must stay in the authenticated tenant');
    return { attempt, forged };
  });

  await step('C4 — الاستمرارية: إعادة تشغيل ثم دخول والتزام المصدر ظاهر بمنشأه', async () => {
    await restartApp();
    await uiJourney.login(ADMIN.email, ADMIN.password);
    await ui.nav(AR.navObligations);
    const body = await page.evaluate(() => document.querySelector('main')?.innerText ?? '');
    assert(body.includes(OBLIGATION_TITLE), 'the obligation did not survive the restart');
    const obligation = withDb((db) => db.prepare('SELECT id, source_type, verification_state, status FROM obligations').get());
    assert(obligation.source_type === 'USER_PROVIDED', 'provenance changed after restart');
    assert(obligation.status === 'OPEN', 'status changed after restart without a transition');
    await shot('prod-12-restart-persistence');
    return { persisted: true, obligation };
  });

  // ---------- D — المهمة والأدلة والمراجعة ----------
  await step('D1 — إنشاء مهمة مرتبطة بالالتزام وإسنادها للموظف', async () => {
    const employeeId = userIdOf(EMPLOYEE.email);
    await ui.nav(AR.navObligations);
    const row = page.locator('#obligations table tbody tr', { hasText: OBLIGATION_TITLE }).first();
    await row.waitFor({ state: 'visible', timeout: 20000 });
    await row.getByRole('button', { name: AR.open, exact: true }).first().click();
    await ui.waitDialog();
    await ui.clickButton(AR.createTask);
    await page.waitForSelector('#create-task-title', { timeout: 25000 });
    const dialog = await ui.waitDialog();
    await uiJourney.fillByLabel(dialog, AR.fieldTaskTitle, TASK_BODY);
    await uiJourney.fillByLabel(dialog, AR.fieldDescription, 'مهمة تنفيذ حقيقية لجمع دليل مطابق على الالتزام.');
    await dialog.locator('select').nth(1).selectOption('HIGH');
    await uiJourney.fillByLabel(dialog, AR.fieldDueDate, '2030-05-31');
    await uiJourney.click(dialog, AR.createTask);
    await ui.waitDialogClosed(25000);
    await sleep(1200);

    await ui.nav(AR.navTasks);
    const taskRow = page.locator('#tasks table tbody tr', { hasText: TASK_BODY }).first();
    await taskRow.waitFor({ state: 'visible', timeout: 20000 });
    const assignSelect = taskRow.locator('select').first();
    await assignSelect.selectOption(employeeId);
    await sleep(400);
    await taskRow.getByRole('button', { name: AR.assignConfirm, exact: true }).first().click();
    await sleep(1800);

    const task = withDb((db) => db.prepare('SELECT id, status, assignee_user_id, obligation_id, version FROM tasks').get());
    assert(task, 'the task row was not created');
    assert(task.assignee_user_id === employeeId, `task not assigned to the employee: ${task.assignee_user_id}`);
    assert(task.obligation_id === report.db.obligation.id, 'task not linked to the production obligation');
    const created = scalar("SELECT COUNT(*) FROM audit_events WHERE event_type = 'TASK_CREATED'");
    assert(created === 1, `expected a TASK_CREATED audit event, got ${created}`);
    await shot('prod-13-task-assigned');
    report.db.task = task;
    return { task, employeeId };
  });

  await step('D2 — الموظف يبدأ المهمة ويسجّل دليلاً حقيقياً', async () => {
    const employeeId = userIdOf(EMPLOYEE.email);
    await uiJourney.logout();
    await uiJourney.login(EMPLOYEE.email, EMPLOYEE.rotatedPassword);
    await ui.nav(AR.navTasks);
    await uiJourney.click(page, AR.transitionStart);
    await ui.waitDialog();
    await ui.applyExpectSuccess();
    await uiJourney.waitStatusInTasks(TASK_BODY, AR.statusInProgress);

    await ui.nav(AR.navEvidence);
    await page.getByRole('button', { name: AR.registerEvidencePage, exact: true }).first().click();
    const dialog = await ui.waitDialog();
    await dialog.locator('select').first().selectOption({ label: OBLIGATION_TITLE });
    await uiJourney.fillByLabel(dialog, AR.fieldEvidenceLabel, 'UAT production evidence reference 2026');
    await uiJourney.fillByLabel(dialog, AR.fieldFingerprint, 'uat-production-fingerprint-2026');
    await uiJourney.click(dialog, AR.registerEvidence);
    await ui.waitDialogClosed(25000);
    await sleep(1500);

    const evidence = evidenceIds();
    assert(evidence.length === 1, `expected exactly one evidence row, got ${evidence.length}`);
    assert(evidence[0].status === 'REQUIRES_REVIEW', `new evidence must require review, got ${evidence[0].status}`);
    assert(evidence[0].registered_by === employeeId, `evidence registered_by must be the employee, got ${evidence[0].registered_by}`);
    assert(evidence[0].obligation_id === report.db.obligation.id, 'evidence not linked to the production obligation');
    const status = scalar('SELECT status FROM tasks WHERE title = ?', TASK_BODY);
    assert(status === 'IN_PROGRESS', `task must be IN_PROGRESS, got ${status}`);
    await shot('prod-14-evidence-registered');
    report.db.evidence = evidence[0];
    return { evidence: evidence[0], taskStatus: status };
  });

  await step('D3 — منع ترقية مقدّم الدليل لحالة التحقق (UAT-04) من الواجهة والخلف', async () => {
    const evidence = evidenceIds()[0];
    const statusSelects = await page.locator('#evidence select.table-status-select').count();
    assert(statusSelects === 0, 'the performer must not see an evidence status control');
    const attempt = await ui.ipc('update_evidence_status', { input: { evidenceId: evidence.id, status: 'VALID' } });
    const after = evidenceIds()[0];
    assert(attempt.ok === false, 'self-verification through IPC unexpectedly succeeded');
    assert(['AUTHORIZATION_DENIED', 'EVIDENCE_SELF_VERIFICATION_FORBIDDEN'].includes(attempt.code), `unexpected rejection code: ${JSON.stringify(attempt)}`);
    assert(after.status === evidence.status && after.version === evidence.version, 'rejected self-verification must not write');
    await shot('prod-15-self-verification-blocked');
    return { attempt, statusSelects, unchanged: true };
  });

  await step('D4 — المراجع المستقل يقرر حالة التحقق من الدليل', async () => {
    await uiJourney.logout();
    await uiJourney.login(REVIEWER.email, REVIEWER.password);
    await ui.nav(AR.navEvidence);
    const statusSelect = page.locator('#evidence select.table-status-select').first();
    await statusSelect.waitFor({ state: 'visible', timeout: 20000 });
    await statusSelect.selectOption('VALID');
    await sleep(2000);
    const evidence = evidenceIds()[0];
    const audit = scalar("SELECT COUNT(*) FROM audit_events WHERE event_type = 'EVIDENCE_STATUS_UPDATED'");
    assert(evidence.status === 'VALID', `reviewer could not set evidence VALID: ${evidence.status}`);
    assert(audit === 1, `expected one EVIDENCE_STATUS_UPDATED event, got ${audit}`);
    await shot('prod-16b-evidence-validated');
    await uiJourney.logout();
    return { evidence, audit };
  });

  await step('D5 — الموظف يطلب المراجعة فتُنشأ موافقة معلّقة', async () => {
    await uiJourney.login(EMPLOYEE.email, EMPLOYEE.rotatedPassword);
    await ui.nav(AR.navTasks);
    await uiJourney.click(page, AR.transitionSubmit);
    await ui.waitDialog();
    await ui.applyExpectSuccess();
    await uiJourney.waitStatusInTasks(TASK_BODY, AR.statusReady);
    const approval = pendingApproval();
    assert(approval, 'review request did not create a pending approval');
    assert(approval.requested_by === userIdOf(EMPLOYEE.email), 'approval requester is not the employee');
    const status = scalar('SELECT status FROM tasks WHERE title = ?', TASK_BODY);
    assert(status === 'READY_FOR_REVIEW', `task must be READY_FOR_REVIEW, got ${status}`);
    await shot('prod-16-ready-for-review');
    return { approval, taskStatus: status };
  });

  await step('D6 — المراجع المستقل يرفض العمل بسبب واضح', async () => {
    await uiJourney.logout();
    await uiJourney.login(REVIEWER.email, REVIEWER.password);
    const approval = pendingApproval();
    await ui.nav(AR.navAttention);
    const decisionButton = page.getByRole('button', { name: AR.decide, exact: true }).first();
    await decisionButton.waitFor({ state: 'visible', timeout: 20000 });
    await decisionButton.click();
    const dialog = await ui.waitDialog();
    await uiJourney.fillByLabel(dialog, AR.fieldReason, REJECT_REASON);
    await uiJourney.click(dialog, AR.reject);
    await ui.waitDialogClosed(25000);
    await sleep(1500);

    const decided = withDb((db) => db.prepare('SELECT decision, decision_reason, decision_by, requested_by FROM approvals WHERE id = ?').get(approval.id));
    assert(decided.decision === 'REJECTED', `approval should be REJECTED, got ${decided.decision}`);
    assert(decided.decision_by === userIdOf(REVIEWER.email), 'rejection must be decided by the reviewer');
    assert(decided.decision_by !== decided.requested_by, 'decision maker must differ from the requester');
    const status = scalar('SELECT status FROM tasks WHERE title = ?', TASK_BODY);
    assert(status === 'REJECTED', `task must be REJECTED, got ${status}`);
    await shot('prod-17-rejected');
    report.db.rejection = decided;
    return { decided, taskStatus: status };
  });

  await step('D7 — الموظف يرى سبب الرفض ويعيد العمل ثم يقدّمه من جديد', async () => {
    await uiJourney.logout();
    await uiJourney.login(EMPLOYEE.email, EMPLOYEE.rotatedPassword);
    await ui.nav(AR.navTasks);
    await uiJourney.waitStatusInTasks(TASK_BODY, AR.statusRejected);
    const rejection = withDb((db) => db.prepare("SELECT reason FROM task_state_transitions WHERE to_status = 'REJECTED' ORDER BY rowid DESC LIMIT 1").get());
    assert(rejection && (rejection.reason ?? '').length > 0, 'rejection reason not recorded in the lifecycle log');
    await page.getByRole('button', { name: AR.history, exact: true }).first().click();
    const historyDialog = await ui.waitDialog();
    let historyText = '';
    const deadline = Date.now() + 20000;
    while (Date.now() < deadline) {
      historyText = (await historyDialog.textContent()) ?? '';
      if (historyText.includes('مرفوض')) break;
      await sleep(300);
    }
    assert(historyText.includes('الدليل المرجعي غير كافٍ'), `rejection reason not visible to the performer: ${historyText.slice(0, 200)}`);
    await page.keyboard.press('Escape');
    await ui.waitDialogClosed();
    await shot('prod-18-rejection-visible');

    await uiJourney.click(page, AR.transitionStart);
    await ui.waitDialog();
    await ui.applyExpectSuccess();
    await uiJourney.waitStatusInTasks(TASK_BODY, AR.statusInProgress);
    await uiJourney.click(page, AR.transitionSubmit);
    await ui.waitDialog();
    await ui.applyExpectSuccess();
    await uiJourney.waitStatusInTasks(TASK_BODY, AR.statusReady);
    const approvals = rows("SELECT id, decision FROM approvals WHERE resource_type='TASK' ORDER BY rowid ASC");
    assert(approvals.length === 2, `expected 2 approvals (rejected + pending), got ${approvals.length}`);
    assert(pendingApproval(), 'rework did not create a new pending approval');
    return { rejectionReason: rejection.reason, approvals: approvals.length, resubmitted: true };
  });

  await step('D8 — المراجع المستقل يعتمد فتبتقل المهمة إلى APPROVED', async () => {
    await uiJourney.logout();
    await uiJourney.login(REVIEWER.email, REVIEWER.password);
    await ui.nav(AR.navAttention);
    const decisionButton = page.getByRole('button', { name: AR.decide, exact: true }).first();
    await decisionButton.waitFor({ state: 'visible', timeout: 20000 });
    await decisionButton.click();
    const dialog = await ui.waitDialog();
    await uiJourney.fillByLabel(dialog, AR.fieldReason, APPROVE_REASON);
    await uiJourney.click(dialog, AR.approve);
    await ui.waitDialogClosed(25000);
    await sleep(1500);
    const status = scalar('SELECT status FROM tasks WHERE title = ?', TASK_BODY);
    assert(status === 'APPROVED', `task must be APPROVED after approval, got ${status}`);
    const approval = withDb((db) => db.prepare("SELECT decision, decision_by FROM approvals WHERE resource_type='TASK' AND decision='APPROVED'").get());
    assert(approval?.decision === 'APPROVED', 'approval not closed as APPROVED');
    await shot('prod-19-approved');
    return { taskStatus: status, approval };
  });

  await step('D9 — الموظف يغلق المهمة فتصبح COMPLETED نهائية', async () => {
    await uiJourney.logout();
    await uiJourney.login(EMPLOYEE.email, EMPLOYEE.rotatedPassword);
    await ui.nav(AR.navTasks);
    await uiJourney.click(page, AR.transitionClose);
    await ui.waitDialog();
    await ui.applyExpectSuccess();
    await uiJourney.waitStatusInTasks(TASK_BODY, AR.statusCompleted);
    const status = scalar('SELECT status FROM tasks WHERE title = ?', TASK_BODY);
    assert(status === 'COMPLETED', `task must be COMPLETED, got ${status}`);
    await shot('prod-20-completed');
    return { taskStatus: status };
  });

  await step('D10 — سجل الانتقالات يطابق الرحلة السباعية بالترتيب', async () => {
    const transitions = rows('SELECT from_status, to_status, reason FROM task_state_transitions ORDER BY rowid ASC');
    const sequence = transitions.map((item) => `${item.from_status}->${item.to_status}`);
    const expected = [
      'OPEN->IN_PROGRESS',
      'IN_PROGRESS->READY_FOR_REVIEW',
      'READY_FOR_REVIEW->REJECTED',
      'REJECTED->IN_PROGRESS',
      'IN_PROGRESS->READY_FOR_REVIEW',
      'READY_FOR_REVIEW->APPROVED',
      'APPROVED->COMPLETED',
    ];
    assert(sequence.length === expected.length, `expected ${expected.length} transitions, got ${sequence.length}: ${sequence.join(',')}`);
    for (let index = 0; index < expected.length; index += 1) {
      assert(sequence[index] === expected[index], `transition ${index}: got ${sequence[index]} expected ${expected[index]}`);
    }
    report.db.transitions = transitions;
    return { count: sequence.length, sequence };
  });

  // ---------- E — النتائج والتقارير والسلبيات ----------
  await step('E1 — المسؤول يتحقق بشرياً من الالتزام ثم تُقاس الجاهزية', async () => {
    await uiJourney.logout();
    await uiJourney.login(ADMIN.email, ADMIN.password);
    await ui.nav(AR.navObligations);
    const row = page.locator('#obligations table tbody tr', { hasText: OBLIGATION_TITLE }).first();
    await row.getByRole('button', { name: AR.open, exact: true }).first().click();
    const dialog = await ui.waitDialog();
    await uiJourney.click(dialog, AR.verifyObligation);
    await sleep(2000);
    await page.keyboard.press('Escape');
    await ui.waitDialogClosed(20000);
    const obligation = withDb((db) => db.prepare('SELECT verification_state, version FROM obligations WHERE id = ?').get(report.db.obligation.id));
    assert(obligation.verification_state === 'HUMAN_VERIFIED', `expected HUMAN_VERIFIED, got ${obligation.verification_state}`);
    const audit = scalar("SELECT COUNT(*) FROM audit_events WHERE event_type = 'OBLIGATION_HUMAN_VERIFIED'");
    assert(audit === 1, `expected one OBLIGATION_HUMAN_VERIFIED event, got ${audit}`);
    return { obligation, audit };
  });

  await step('E2 — اللوحة التنفيذية والمؤشرات تطابق صفوف قاعدة الاختبار', async () => {
    await ui.nav(AR.navOverview);
    await sleep(1500);
    const db = {
      total: scalar('SELECT COUNT(*) FROM tasks'),
      open: scalar("SELECT COUNT(*) FROM tasks WHERE status='OPEN'"),
      inProgress: scalar("SELECT COUNT(*) FROM tasks WHERE status='IN_PROGRESS'"),
      ready: scalar("SELECT COUNT(*) FROM tasks WHERE status='READY_FOR_REVIEW'"),
      rejected: scalar("SELECT COUNT(*) FROM tasks WHERE status='REJECTED'"),
      completed: scalar("SELECT COUNT(*) FROM tasks WHERE status='COMPLETED'"),
      overdue: scalar("SELECT COUNT(*) FROM tasks WHERE due_at IS NOT NULL AND due_at < ? AND status <> 'COMPLETED'", new Date().toISOString()),
      obligations: scalar('SELECT COUNT(*) FROM obligations WHERE archived_at IS NULL'),
      evidence: scalar('SELECT COUNT(*) FROM evidence_items'),
    };
    const body = await page.evaluate(() => document.querySelector('main')?.innerText ?? '');
    const chainValid = body.includes(AR.chainValid);
    assert(chainValid, 'the dashboard must report the audit chain as intact');
    const eligible = rows("SELECT id, status, verification_state FROM obligations WHERE applicability = 'APPLIES' AND archived_at IS NULL");
    const covered = new Set(rows("SELECT DISTINCT obligation_id FROM evidence_items WHERE status IN ('UPLOADED','PROCESSING','ANALYZED','VALID')").map((item) => item.obligation_id));
    const humanVerified = eligible.filter((item) => item.verification_state === 'HUMAN_VERIFIED').length;
    const evidenceCovered = eligible.filter((item) => covered.has(item.id)).length;
    const coverage = eligible.length ? Math.floor((humanVerified * 100) / eligible.length) : null;
    const completeness = eligible.length ? Math.floor((evidenceCovered * 100) / eligible.length) : null;
    const score = coverage === null ? null : Math.floor((coverage * 60 + completeness * 40) / 100);
    const readinessUi = await page.locator('section[aria-label="تقدير الجاهزية"] .metric-card').evaluateAll((nodes) => nodes.map((node) => ({
      value: (node.querySelector('.readiness-ring span')?.textContent ?? node.querySelector('strong')?.textContent ?? '').trim(),
    })));
    // بلا التزام مؤهل: يجب أن تُعلن الواجهة «غير مقيس» لا نسبة مخترعة.
    if (eligible.length === 0) {
      assert(
        readinessUi[0]?.value === '—' || readinessUi[0]?.value === AR.readinessNotMeasured,
        `no eligible obligations must yield "not measured", got "${readinessUi[0]?.value}"`,
      );
    } else {
      assert(readinessUi[0]?.value === `${score}%`, `readiness score mismatch: UI="${readinessUi[0]?.value}" expected="${score}%"`);
      assert(readinessUi[1]?.value === `${coverage}%`, `coverage mismatch: UI="${readinessUi[1]?.value}" expected="${coverage}%"`);
      assert(readinessUi[2]?.value === `${completeness}%`, `completeness mismatch: UI="${readinessUi[2]?.value}" expected="${completeness}%"`);
    }
    await ui.nav(AR.navTasks);
    const cards = await page.locator('#tasks .metric-card').evaluateAll((nodes) =>
      nodes.map((node) => ({ label: (node.querySelector('span')?.textContent ?? '').trim(), value: (node.querySelector('strong')?.textContent ?? '').trim() })));
    const readCard = (fragment) => {
      const card = cards.find((item) => item.label.includes(fragment));
      return card ? Number(card.value) : null;
    };
    const uiValues = {
      total: readCard('إجمالي المهام'),
      open: readCard('المهام المفتوحة'),
      inProgress: readCard('المهام قيد التنفيذ'),
      ready: readCard('بانتظار المراجعة'),
      rejected: readCard('مرفوضة'),
      completed: readCard('المهام المكتملة'),
      overdue: readCard('متأخرة'),
    };
    for (const key of ['total', 'open', 'inProgress', 'ready', 'rejected', 'completed', 'overdue']) {
      assert(uiValues[key] === db[key], `metric ${key}: UI=${uiValues[key]} DB=${db[key]}`);
    }
    report.db.dashboard = { db, uiValues, readiness: { eligible: eligible.length, humanVerified, evidenceCovered, coverage, completeness, score } };
    await ui.nav(AR.navOverview);
    await shot('prod-21-dashboard');
    return report.db.dashboard;
  });

  await step('E3 — الوحدات تعرض بيانات المؤسسة نفسها لا بيانات ديمو', async () => {
    const checks = {};
    for (const [label, nav] of [['attention', AR.navAttention], ['obligations', AR.navObligations], ['tasks', AR.navTasks], ['evidence', AR.navEvidence], ['controls', AR.navControls], ['audit', AR.navAudit]]) {
      await ui.nav(nav);
      const body = await page.evaluate(() => document.querySelector('main')?.innerText ?? '');
      checks[label] = body.length;
      assert(body.length > 120, `module ${label} rendered empty content`);
      assert(!body.includes('بيانات تجريبية / تركيبية'), `module ${label} must not present demo labelling in a production workspace`);
    }
    await ui.nav(AR.navTasks);
    const taskBody = await page.evaluate(() => document.querySelector('main')?.innerText ?? '');
    assert(taskBody.includes(TASK_BODY) && taskBody.includes(AR.statusCompleted), 'task list does not reflect the completed production task');
    await shot('prod-22-modules');
    return checks;
  });

  await step('E4 — التقرير يُبنى من سجلات المؤسسة ويطابق عدّ القاعدة', async () => {
    await ui.nav(AR.navReports);
    await page.waitForSelector('#reports-catalog-title', { timeout: 25000 });
    const catalogCount = await page.locator('.report-catalog__item').count();
    assert(catalogCount === 6, `report catalogue must expose 6 definitions, got ${catalogCount}`);
    await page.locator('.report-catalog__item', { hasText: 'عبء المهام' }).first().click();
    await page.waitForSelector('#reports-result-title', { timeout: 25000 });
    await sleep(2000);
    const body = await page.evaluate(() => document.querySelector('main')?.innerText ?? '');
    assert(body.includes('لحظة التوليد') && body.includes('الجداول المصدر'), 'report provenance block missing');
    assert(body.includes('ليس إقراراً بالامتثال القانوني'), 'report must state it is not a compliance certification');
    assert(body.includes(EMPLOYEE.name), 'workload report does not show the real assignee from the database');
    const viaIpc = await ui.ipc('generate_report', { input: { reportId: 'TASK_WORKLOAD' } });
    assert(viaIpc.ok, `generate_report failed: ${JSON.stringify(viaIpc)}`);
    const workload = (viaIpc.result.sections ?? []).find((section) => section.columns.some((column) => column.key === 'assigneeUserId') && section.columns.some((column) => column.key === 'totalTasks'));
    assert(workload, 'TASK_WORKLOAD report has no assignee section');
    const columnIndex = (key) => workload.columns.findIndex((column) => column.key === key);
    const employeeRow = workload.rows.find((row) => row[columnIndex('assigneeUserId')] === userIdOf(EMPLOYEE.email));
    assert(employeeRow, `report has no row for the real assignee; rows=${JSON.stringify(workload.rows).slice(0, 300)}`);
    const dbTasks = scalar('SELECT COUNT(*) FROM tasks');
    const dbCompleted = scalar("SELECT COUNT(*) FROM tasks WHERE status = 'COMPLETED'");
    const reportTotal = Number(employeeRow[columnIndex('totalTasks')]);
    const reportCompleted = Number(employeeRow[columnIndex('completedTasks')]);
    assert(reportTotal === dbTasks, `report total ${reportTotal} != DB tasks ${dbTasks}`);
    assert(reportCompleted === dbCompleted, `report completed ${reportCompleted} != DB completed ${dbCompleted}`);
    await shot('prod-23-report');
    return { catalogCount, reportTotal, reportCompleted, dbTasks, dbCompleted };
  });

  await step('E5 — تصدير CSV عبر حوار الحفظ الأصلي: يفتح الحوار والإلغاء لا يترك ملفاً (UAT-06)', async () => {
    const exportEventsBefore = scalar("SELECT COUNT(*) FROM audit_events WHERE event_type = 'REPORT_EXPORTED'");
    const button = page.getByRole('button', { name: AR.exportCsv, exact: true }).first();
    await button.waitFor({ state: 'visible', timeout: 20000 });
    await button.click();
    await sleep(2500);
    const probe = nativeSaveDialog.probe(true);
    await sleep(1500);
    const exportEventsAfter = scalar("SELECT COUNT(*) FROM audit_events WHERE event_type = 'REPORT_EXPORTED'");
    report.db.nativeSaveDialog = { probe, exportEventsBefore, exportEventsAfter };
    assert(probe.detected, `the native Windows save dialog did not open (probe: ${JSON.stringify(probe)})`);
    assert(exportEventsAfter === exportEventsBefore, 'cancelling the save dialog must not write a file nor audit an export');
    report.limitations.push('UAT-06: حوار الحفظ الأصلي فُتح فعلياً وأُلغي بلا ملف (مقاس)، لكن اختيار وجهة وحفظ الملف داخل الحوار غير قابل للأتمتة عبر CDP — مسار الكتابة تحقق بأمر الخلف نفسه أدناه.');
    const outDir = EVIDENCE ? path.resolve(EVIDENCE) : path.resolve('.tmp-uat');
    const csvPath = path.join(outDir, 'uat-production-report.csv');
    const mdPath = path.join(outDir, 'uat-production-report.md');
    const csv = await ui.ipc('export_report', { input: { reportId: 'TASK_WORKLOAD', format: 'CSV', targetPath: csvPath } });
    const md = await ui.ipc('export_report', { input: { reportId: 'TASK_WORKLOAD', format: 'MARKDOWN', targetPath: mdPath } });
    assert(csv.ok, `CSV export failed: ${JSON.stringify(csv)}`);
    assert(md.ok, `Markdown export failed: ${JSON.stringify(md)}`);
    const { readFileSync } = await import('node:fs');
    const csvText = readFileSync(csvPath, 'utf8');
    const mdText = readFileSync(mdPath, 'utf8');
    assert(csvText.length > 40 && mdText.length > 100, 'exported files are empty');
    assert(mdText.includes('NEXUS') || mdText.includes('تقرير'), 'markdown export missing provenance header');
    const exportEvents = scalar("SELECT COUNT(*) FROM audit_events WHERE event_type = 'REPORT_EXPORTED'");
    assert(exportEvents === exportEventsBefore + 2, `expected two REPORT_EXPORTED events, got ${exportEvents - exportEventsBefore}`);
    report.db.reportExport = { csvBytes: csvText.length, mdBytes: mdText.length, exportEvents };
    await shot('prod-24-export');
    return report.db.reportExport;
  });

  await step('E6 — رفض وجهة تصدير غير آمنة', async () => {
    const relative = await ui.ipc('export_report', { input: { reportId: 'TASK_WORKLOAD', format: 'CSV', targetPath: 'uat-relative.csv' } });
    const wrongExtension = await ui.ipc('export_report', { input: { reportId: 'TASK_WORKLOAD', format: 'CSV', targetPath: 'C:\\Windows\\System32\\uat-export.txt' } });
    assert(relative.ok === false, 'a relative export destination must be rejected');
    assert(wrongExtension.ok === false, 'an unsafe export destination must be rejected');
    return { relative, wrongExtension };
  });

  await step('E7 — سلبيات خلفية على قاعدة الإنتاج: منشأ وحقن وتزامن وتجاوز', async () => {
    const before = {
      obligations: scalar('SELECT COUNT(*) FROM obligations'),
      users: scalar('SELECT COUNT(*) FROM local_users'),
      tasks: scalar('SELECT COUNT(*) FROM tasks'),
      approvals: scalar('SELECT COUNT(*) FROM approvals'),
    };
    const results = {};

    const injectionTitle = "UAT'); DROP TABLE obligations;--";
    const injection = await ui.ipc('create_obligation', {
      input: { sourceId: report.db.regulatorySource.id, title: injectionTitle, description: '', owner: 'Owner', dueDate: null, priority: 'LOW', applicability: 'UNCERTAIN' },
    });
    results.injection = injection;
    assert(injection.ok === true, `injection payload obligation should be stored literally: ${JSON.stringify(injection)}`);
    const storedTitle = scalar('SELECT title FROM obligations WHERE id = ?', injection.result.id);
    assert(storedTitle === injectionTitle, 'the SQL payload was not stored literally');
    assert(scalar('SELECT COUNT(*) FROM obligations') === before.obligations + 1, 'obligations table integrity was affected');
    results.archiveInjection = await ui.ipc('archive_obligation', { input: { obligationId: injection.result.id, expectedVersion: 1 } });

    results.foreignSource = await ui.ipc('create_obligation', {
      input: { sourceId: 'source_from_another_tenant', title: 'Cross tenant', description: '', owner: 'Owner', dueDate: null, priority: 'LOW', applicability: 'APPLIES' },
    });
    assert(results.foreignSource.ok === false && results.foreignSource.code === 'OBLIGATION_SOURCE_INVALID', `wrong foreign source rejection: ${JSON.stringify(results.foreignSource)}`);

    results.invalidInput = await ui.ipc('create_obligation', {
      input: { sourceId: report.db.regulatorySource.id, title: '   ', description: '', owner: 'Owner', dueDate: null, priority: 'LOW', applicability: 'APPLIES' },
    });
    assert(results.invalidInput.ok === false && results.invalidInput.code === 'VALIDATION_FAILED', `wrong invalid input rejection: ${JSON.stringify(results.invalidInput)}`);

    results.unsafeUrl = await ui.ipc('create_regulatory_source', {
      input: { authority: 'Unsafe', documentTitle: 'Unsafe', documentVersion: null, sourceUrl: 'javascript:alert(1)' },
    });
    assert(results.unsafeUrl.ok === false && results.unsafeUrl.code === 'REGULATORY_SOURCE_INVALID', `wrong unsafe URL rejection: ${JSON.stringify(results.unsafeUrl)}`);

    results.duplicateSource = await ui.ipc('create_regulatory_source', {
      input: { authority: SOURCE.authority, documentTitle: SOURCE.document, documentVersion: SOURCE.version, sourceUrl: SOURCE.url },
    });
    assert(results.duplicateSource.ok === false && results.duplicateSource.code === 'REGULATORY_SOURCE_DUPLICATE', `wrong duplicate rejection: ${JSON.stringify(results.duplicateSource)}`);

    const concurrent = await page.evaluate(async (payload) => {
      const invoke = (email) => window.__TAURI_INTERNALS__.invoke('create_workspace_user', {
        input: { displayName: 'Concurrent Member', email, initialPassword: payload.password, roleCodes: ['CONTRIBUTOR'] },
      }).then(() => 'OK').catch((error) => (error && error.code) || 'UNKNOWN');
      return Promise.all([invoke(payload.email), invoke(payload.email)]);
    }, { email: 'uat.concurrent@example.test', password: 'C0ncurrent!Passw0rd#26' });
    results.concurrent = concurrent;
    const concurrentRows = scalar("SELECT COUNT(*) FROM local_users WHERE email = 'uat.concurrent@example.test'");
    assert(concurrentRows === 1, `concurrent creation must leave exactly one account, got ${concurrentRows}`);
    assert(concurrent.filter((value) => value === 'OK').length === 1, `exactly one concurrent creation must succeed: ${JSON.stringify(concurrent)}`);
    assert(concurrent.some((value) => value === 'USER_EMAIL_EXISTS'), `the losing concurrent request must be rejected: ${JSON.stringify(concurrent)}`);

    const approved = withDb((db) => db.prepare("SELECT id FROM approvals WHERE resource_type='TASK' AND decision='APPROVED' ORDER BY rowid DESC LIMIT 1").get());
    results.duplicateApproval = await ui.ipc('decide_approval', { input: { approvalId: approved.id, decision: 'APPROVED', reason: 'duplicate' } });
    assert(results.duplicateApproval.ok === false, 'duplicate approval decision must be refused');
    results.directCompleted = await ui.ipc('transition_task', { input: { taskId: taskId(), targetStatus: 'IN_PROGRESS', reason: 'reopen', expectedVersion: scalar('SELECT version FROM tasks WHERE id = ?', taskId()) } });
    assert(results.directCompleted.ok === false, 'a completed task must stay terminal');
    results.staleTransition = await ui.ipc('transition_task', { input: { taskId: taskId(), targetStatus: 'COMPLETED', reason: 'stale', expectedVersion: 1 } });
    assert(results.staleTransition.ok === false, 'a stale expected version must be refused');

    const after = {
      obligations: scalar('SELECT COUNT(*) FROM obligations'),
      users: scalar('SELECT COUNT(*) FROM local_users'),
      tasks: scalar('SELECT COUNT(*) FROM tasks'),
      approvals: scalar('SELECT COUNT(*) FROM approvals'),
    };
    assert(after.tasks === before.tasks && after.approvals === before.approvals, `rejected negatives must not write: before=${JSON.stringify(before)} after=${JSON.stringify(after)}`);
    assert(after.users === before.users + 1, 'only the one concurrent account may be added');
    assert(after.obligations === before.obligations + 1, 'only the literal-injection obligation may be added');
    report.db.negatives = { results, before, after };
    await shot('prod-25-negatives');
    return { results, before, after };
  });

  await step('E8 — تحقق فعلي من سلسلة التدقيق عبر أمر الخلف', async () => {
    const verification = await ui.ipc('verify_audit_chain');
    assert(verification.ok, `verify_audit_chain failed: ${JSON.stringify(verification)}`);
    const { valid, events, brokenLinks } = verification.result;
    report.db.auditVerification = verification.result;
    assert(valid === true, 'the audit chain must verify as valid');
    assert(brokenLinks === 0, `audit chain has ${brokenLinks} broken links`);
    assert(events > 20, `expected the full journey to be audited, got ${events} events`);
    const chain = auditChainCheck();
    assert(chain.brokenLinks === 0, 'independent raw chain check disagrees with the backend');
    return { valid, events, brokenLinks };
  });

  await step('E9 — إعادة تشغيل أخيرة: البيانات والحالات والتدقيق باقية', async () => {
    const beforeRestart = {
      obligationStatus: scalar('SELECT status FROM obligations WHERE id = ?', report.db.obligation.id),
      obligationVerification: scalar('SELECT verification_state FROM obligations WHERE id = ?', report.db.obligation.id),
      taskStatus: scalar('SELECT status FROM tasks WHERE id = ?', report.db.task.id),
      evidenceStatus: scalar('SELECT status FROM evidence_items WHERE obligation_id = ?', report.db.obligation.id),
      approvals: scalar('SELECT COUNT(*) FROM approvals'),
      users: scalar('SELECT COUNT(*) FROM local_users'),
      events: scalar('SELECT COUNT(*) FROM audit_events'),
    };
    await restartApp();
    await uiJourney.login(ADMIN.email, ADMIN.password);
    await ui.nav(AR.navTasks);
    const body = await page.evaluate(() => document.querySelector('main')?.innerText ?? '');
    assert(body.includes(TASK_BODY) && body.includes(AR.statusCompleted), 'the completed task did not survive the restart');
    const afterRestart = {
      obligationStatus: scalar('SELECT status FROM obligations WHERE id = ?', report.db.obligation.id),
      obligationVerification: scalar('SELECT verification_state FROM obligations WHERE id = ?', report.db.obligation.id),
      taskStatus: scalar('SELECT status FROM tasks WHERE id = ?', report.db.task.id),
      evidenceStatus: scalar('SELECT status FROM evidence_items WHERE obligation_id = ?', report.db.obligation.id),
      approvals: scalar('SELECT COUNT(*) FROM approvals'),
      users: scalar('SELECT COUNT(*) FROM local_users'),
    };
    assert(afterRestart.taskStatus === 'COMPLETED', 'task state lost after restart');
    assert(afterRestart.obligationVerification === 'HUMAN_VERIFIED', 'obligation verification lost after restart');
    assert(afterRestart.evidenceStatus === 'VALID', `evidence state changed after restart: ${afterRestart.evidenceStatus}`);
    assert(afterRestart.users === beforeRestart.users && afterRestart.approvals === beforeRestart.approvals, 'rows lost after restart');
    const chain = auditChainCheck();
    assert(chain.brokenLinks === 0, 'audit chain broken after the final restart');
    report.db.afterRestart = { beforeRestart, afterRestart, chain };
    await shot('prod-26-final-restart');
    return report.db.afterRestart;
  });

  await step('E10 — سلامة قاعدة الإنتاج: تكامل وقيود أجنبية وعزل مستأجرين', async () => {
    const integrity = withDb((db) => db.prepare('PRAGMA integrity_check').get());
    const foreignKeyViolations = withDb((db) => db.prepare('PRAGMA foreign_key_check').all());
    const crossTenantTasks = scalar("SELECT COUNT(*) FROM tasks t INNER JOIN obligations o ON o.id = t.obligation_id WHERE o.tenant_id <> t.tenant_id");
    const orphanApprovals = scalar("SELECT COUNT(*) FROM approvals a WHERE NOT EXISTS (SELECT 1 FROM tasks t WHERE t.id = a.resource_id AND t.tenant_id = a.tenant_id)");
    const chain = auditChainCheck();
    const snapshot = {
      integrity: Object.values(integrity)[0],
      foreignKeyViolations: foreignKeyViolations.length,
      crossTenantTasks,
      orphanApprovals,
      chain,
      tenants: scalar('SELECT COUNT(*) FROM tenants'),
      obligations: scalar('SELECT COUNT(*) FROM obligations'),
      tasks: scalar('SELECT COUNT(*) FROM tasks'),
      evidence: scalar('SELECT COUNT(*) FROM evidence_items'),
      approvals: scalar('SELECT COUNT(*) FROM approvals'),
      auditEvents: scalar('SELECT COUNT(*) FROM audit_events'),
    };
    report.db.integrity = snapshot;
    assert(String(snapshot.integrity) === 'ok', `integrity_check: ${snapshot.integrity}`);
    assert(snapshot.foreignKeyViolations === 0, 'foreign key violations found');
    assert(snapshot.crossTenantTasks === 0, 'cross-tenant task rows found');
    assert(snapshot.orphanApprovals === 0, 'orphan approvals found');
    assert(snapshot.tenants === 1, 'the isolated production database must hold exactly one tenant');
    assert(snapshot.chain.brokenLinks === 0, 'audit chain is broken');
    return snapshot;
  });
};

// =====================================================================
// مرحلة دورة العمل التجريبية: التزام ← مهمة ← إسناد ← دليل ← رفض ← إعادة عمل ← اعتماد ← إغلاق
// =====================================================================
const runDemoPhase = async () => {
  const taskId = () => scalar("SELECT id FROM tasks WHERE title = ?", TASK_TITLE);
  const pendingApproval = () => withDb((db) => db.prepare("SELECT id, resource_version, requested_by, decision FROM approvals WHERE resource_type='TASK' AND decision='PENDING' ORDER BY rowid DESC LIMIT 1").get());

  await step('D1 — بدء التجربة التجريبية من شاشة التأسيس', async () => {
    await page.waitForSelector('#setup-title', { timeout: 60000 });
    await page.getByRole('button', { name: AR.startDemo, exact: true }).first().click();
    await page.waitForSelector('nav.navigation', { timeout: 60000 });
    const mode = scalar('SELECT mode FROM app_runtime_state WHERE id = 1');
    assert(mode === 'DEMO', `runtime mode should be DEMO, got ${mode}`);
    await shot('demo-01-shell');
    return { runtimeMode: mode };
  });

  await step('D2 — بيانات الديمو التركيبية ظاهرة وموسومة ولا سجلات إنتاج مختلقة', async () => {
    const snapshot = {
      tenants: rows('SELECT id, data_mode FROM tenants'),
      obligations: scalar('SELECT COUNT(*) FROM obligations'),
      tasks: scalar('SELECT COUNT(*) FROM tasks'),
      users: scalar('SELECT COUNT(*) FROM local_users'),
      actors: rows('SELECT id FROM local_users ORDER BY id'),
      runtimeMode: scalar('SELECT mode FROM app_runtime_state WHERE id = 1'),
    };
    report.db.demoInitial = snapshot;
    assert(snapshot.runtimeMode === 'DEMO', 'runtime mode not DEMO');
    assert(snapshot.actors.length >= 3, `expected 3 demo actors, got ${snapshot.actors.length}`);
    assert(snapshot.tasks >= 1, 'seeded demo task missing');
    // فاعلا الديمو المعلنان: طالب ومراجِع — المسؤول القديم ليس فاعلاً قابلاً للتبديل.
    const optionCount = await page.locator('.actor-switcher select option').count();
    const optionTexts = await page.locator('.actor-switcher select option').allTextContents();
    assert(optionCount === 2, `actor switcher must expose exactly the two declared demo actors, got ${optionCount}`);
    assert(optionTexts.some((text) => text.includes('طالب')) && optionTexts.some((text) => text.includes('مراجِع')), `demo actors missing responsibilities: ${optionTexts.join(' | ')}`);
    const body = await page.evaluate(() => document.body.innerText);
    assert(body.includes('بيانات تجريبية / تركيبية'), 'demo data must be visibly labelled as synthetic');
    return { ...snapshot, actorOptions: optionCount };
  });

  await step('D3 — الموظف يرى المهمة المسندة إليه ويفتح تفاصيلها', async () => {
    await ui.selectActor('user_demo_operator');
    await ui.nav(AR.navTasks);
    const row = await ui.taskRow();
    await row.waitFor({ state: 'visible', timeout: 20000 });
    const text = (await row.textContent()) ?? '';
    assert(text.includes(TASK_TITLE), 'task title not visible');
    assert(text.includes(AR.statusOpen), `task should start OPEN, row: ${text.slice(0, 200)}`);
    assert(text.includes('NEXUS Demo Operator'), 'task should be assigned to the demo operator');
    assert(text.includes(OBLIGATION_TITLE), 'task must show its obligation binding');
    await page.getByRole('button', { name: AR.history, exact: true }).first().click();
    const historyDialog = await ui.waitDialog();
    const historyText = (await historyDialog.textContent()) ?? '';
    await page.keyboard.press('Escape');
    await ui.waitDialogClosed();
    await shot('demo-02-task-open');
    return { rowExcerpt: text.slice(0, 260), historyEmptyInitially: historyText.includes('لا توجد') || historyText.length > 0 };
  });

  await step('D4 — شرط الحالة: لا يمكن طلب المراجعة بلا دليل مسجَّل (رفض خلفي حقيقي)', async () => {
    await ui.clickButton(AR.transitionStart);
    await ui.waitDialog();
    await ui.applyExpectSuccess();
    await ui.waitRowStatus(AR.statusInProgress);
    // محاولة طلب المراجعة قبل تسجيل الدليل
    await ui.clickButton(AR.transitionSubmit);
    const dialog = await ui.waitDialog();
    await ui.clickButton(AR.applyAction, dialog);
    await sleep(1200);
    const error = await ui.formError();
    assert(error.length > 0, 'backend should reject review request without evidence, but no error was shown');
    assert(error.includes('دليل'), `unexpected error text: ${error}`);
    await page.keyboard.press('Escape');
    await ui.waitDialogClosed();
    const status = scalar('SELECT status FROM tasks WHERE id = ?', taskId());
    assert(status === 'IN_PROGRESS', `task must remain IN_PROGRESS after rejected transition, got ${status}`);
    await shot('demo-03-evidence-required');
    return { rejectionMessage: error, statusAfter: status };
  });

  await step('D5 — تسجيل دليل مرجعي وربطه بالتزام المهمة من واجهة الأدلة', async () => {
    await ui.nav(AR.navEvidence);
    await ui.clickButton(AR.registerEvidencePage);
    const dialog = await ui.waitDialog();
    await dialog.locator('select').first().selectOption({ label: OBLIGATION_TITLE });
    await dialog.getByLabel(AR.fieldEvidenceLabel).fill('UAT synthetic evidence reference 2026');
    await dialog.getByLabel(AR.fieldFingerprint).fill('uat-fingerprint-2026-10-18-synthetic');
    await ui.clickButton(AR.registerEvidence, dialog);
    await sleep(1500);
    await ui.waitDialogClosed();
    const feedback = await ui.feedback();
    const evidence = rows('SELECT id, label, classification, status, obligation_id FROM evidence_items');
    assert(evidence.length >= 1, 'no evidence row created');
    const registered = evidence[evidence.length - 1];
    assert(registered.obligation_id === 'obl_demo_001', 'evidence not linked to the task obligation');
    assert(registered.status === 'REQUIRES_REVIEW', `registered evidence must start as REQUIRES_REVIEW, got ${registered.status}`);
    await shot('demo-04-evidence');
    return { feedback, evidenceCount: evidence.length, registered };
  });

  await step('D5b — قرار حالة التحقق يملكه المراجع المستقل لا مقدّم الدليل (UAT-04)', async () => {
    // 0026: من قدّم الدليل لا يقرر حالته. الواجهة لا تعرض القائمة له أصلاً.
    const performerControls = await page.locator('#evidence select.table-status-select').count();
    assert(performerControls === 0, 'the evidence registrant must not see a verification status control');
    const selfAttempt = await ui.ipc('update_evidence_status', { input: { evidenceId: withDb((db) => db.prepare('SELECT id FROM evidence_items ORDER BY rowid DESC LIMIT 1').get()).id, status: 'VALID' } });
    assert(selfAttempt.ok === false, 'self verification through IPC must be refused');
    // ثم يقرر الحالة مراجع مستقل داخل المؤسسة نفسها.
    await ui.selectActor('user_demo_reviewer');
    await ui.nav(AR.navEvidence);
    const statusSelect = page.locator('#evidence select.table-status-select').first();
    await statusSelect.waitFor({ state: 'visible', timeout: 15000 });
    const targetLabel = await statusSelect.getAttribute('aria-label');
    await statusSelect.selectOption('VALID');
    await sleep(1800);
    const updated = withDb((db) => db.prepare('SELECT id, status FROM evidence_items ORDER BY rowid DESC LIMIT 1').get());
    const obligation = withDb((db) => db.prepare("SELECT id, evidence_status FROM obligations WHERE id = 'obl_demo_001'").get());
    report.db.evidence = { registeredStatus: 'REQUIRES_REVIEW', updatedStatus: updated.status, obligationEvidenceStatus: obligation.evidence_status };
    assert(updated.status === 'VALID', `evidence status should be VALID after authorized change, got ${updated.status}`);
    assert(obligation.evidence_status !== 'MISSING', 'obligation evidence state did not react to the real evidence row');
    await shot('demo-04b-evidence-validated');
    // العودة إلى منفّذ العمل لمتابعة دورة التقديم.
    await ui.selectActor('user_demo_operator');
    return { statusSelectLabel: targetLabel, updated, obligation, selfAttempt };
  });

  await step('D6 — الموظف يقدّم العمل للمراجعة ⇒ تُنشأ موافقة معلّقة وحالة READY_FOR_REVIEW', async () => {
    await ui.nav(AR.navTasks);
    await ui.clickButton(AR.transitionSubmit);
    await ui.waitDialog();
    await ui.applyExpectSuccess();
    await ui.waitRowStatus(AR.statusReady);
    const approval = pendingApproval();
    assert(approval, 'no pending approval created by the review request');
    const status = scalar('SELECT status FROM tasks WHERE id = ?', taskId());
    assert(status === 'READY_FOR_REVIEW', `task status should be READY_FOR_REVIEW, got ${status}`);
    await shot('demo-05-ready-for-review');
    return { taskStatus: status, approval };
  });

  await step('D7 — منع الاعتماد الذاتي: طالب الاعتماد لا يستطيع قراره ولو عبر IPC مباشر', async () => {
    const approval = pendingApproval();
    assert(approval, 'pending approval missing');
    const attempt = await ui.ipc('decide_approval', {
      input: { approvalId: approval.id, decision: 'APPROVED', reason: 'self approval attempt' },
    });
    assert(attempt.ok === false, 'self/ineligible approval unexpectedly succeeded');
    const decision = scalar('SELECT decision FROM approvals WHERE id = ?', approval.id);
    assert(decision === 'PENDING', `approval must stay PENDING, got ${decision}`);
    return { attempt, approvalStillPending: decision };
  });

  await step('D8 — المراجع يرفض العمل بسبب واضح ويظهر السبب للموظف', async () => {
    await ui.selectActor('user_demo_reviewer');
    await ui.nav(AR.navAttention);
    const decisionButton = page.getByRole('button', { name: 'اتخاذ قرار', exact: true }).first();
    const hasDecisionButton = await decisionButton.count();
    assert(hasDecisionButton > 0, 'reviewer cannot see a decision action for the pending approval');
    await decisionButton.click();
    const dialog = await ui.waitDialog();
    await dialog.getByLabel(AR.fieldReason).fill('UAT: الدليل المرجعي غير كافٍ — أعد ربط دليل مطابق.');
    await ui.clickButton(AR.reject, dialog);
    await sleep(1500);
    await ui.waitDialogClosed();
    const approval = withDb((db) => db.prepare("SELECT id, decision, decision_reason FROM approvals WHERE resource_type='TASK' ORDER BY rowid DESC LIMIT 1").get());
    assert(approval.decision === 'REJECTED', `approval should be REJECTED, got ${approval.decision}`);
    const status = scalar('SELECT status FROM tasks WHERE id = ?', taskId());
    assert(status === 'REJECTED', `task must move to REJECTED after rejection, got ${status}`);
    await shot('demo-06-rejected');
    return { approval, taskStatus: status };
  });

  await step('D9 — الموظف يرى سبب الرفض ويبدأ إعادة العمل', async () => {
    await ui.selectActor('user_demo_operator');
    await ui.nav(AR.navTasks);
    await ui.waitRowStatus(AR.statusRejected);
    // سجل الانتقالات يجب أن يحمل سبب الرفض
    const transitions = rows('SELECT from_status, to_status, reason FROM task_state_transitions ORDER BY created_at ASC');
    const rejection = transitions.find((item) => item.to_status === 'REJECTED');
    assert(rejection, 'rejection transition missing from the lifecycle log');
    assert((rejection.reason ?? '').length > 0, 'rejection reason not recorded in the transition log');
    await page.getByRole('button', { name: AR.history, exact: true }).first().click();
    const historyDialog = await ui.waitDialog();
    // السجل يُحمَّل من الخلف: ننتظر ظهور بنود الانتقالات قبل قراءة النص.
    let historyText = '';
    const historyDeadline = Date.now() + 20000;
    while (Date.now() < historyDeadline) {
      historyText = (await historyDialog.textContent()) ?? '';
      if (historyText.includes('مرفوض') || historyText.includes('بانتظار')) break;
      await sleep(300);
    }
    await page.keyboard.press('Escape');
    await ui.waitDialogClosed();
    assert(historyText.includes('الدليل المرجعي غير كافٍ'), `rejection reason not visible to the performer in the UI; history text: ${historyText.slice(0, 300)}`);
    await shot('demo-07-rework');
    return { rejectionReason: rejection.reason, visibleToPerformer: true };
  });

  await step('D10 — إعادة العمل ثم إعادة التقديم تُنشئ موافقة جديدة', async () => {
    await ui.clickButton(AR.transitionStart);
    await ui.waitDialog();
    await ui.applyExpectSuccess();
    await ui.waitRowStatus(AR.statusInProgress);
    await ui.clickButton(AR.transitionSubmit);
    await ui.waitDialog();
    await ui.applyExpectSuccess();
    await ui.waitRowStatus(AR.statusReady);
    const approvals = rows("SELECT id, decision FROM approvals WHERE resource_type='TASK' ORDER BY rowid ASC");
    const pending = pendingApproval();
    assert(pending, 'rework did not create a new pending approval');
    assert(approvals.length === 2, `expected 2 approvals (rejected + pending), got ${approvals.length}`);
    return { approvals: approvals.length, pending, taskStatus: 'READY_FOR_REVIEW' };
  });

  await step('D11 — المراجع المستقل يعتمد العمل فيتغيّر وضع المهمة فعلياً', async () => {
    await ui.selectActor('user_demo_reviewer');
    await ui.nav(AR.navAttention);
    const decisionButton = page.getByRole('button', { name: 'اتخاذ قرار', exact: true }).first();
    await decisionButton.click();
    const dialog = await ui.waitDialog();
    await dialog.getByLabel(AR.fieldReason).fill('UAT: الدليل مطابق والتحقق مكتمل — اعتماد.');
    await ui.clickButton(AR.approve, dialog);
    await sleep(1500);
    await ui.waitDialogClosed();
    const status = scalar('SELECT status FROM tasks WHERE id = ?', taskId());
    assert(status === 'APPROVED', `task must be APPROVED after approval, got ${status}`);
    const approval = withDb((db) => db.prepare("SELECT decision, decision_by FROM approvals WHERE resource_type='TASK' AND decision='APPROVED'").get());
    assert(approval?.decision === 'APPROVED', 'approval not closed as APPROVED');
    await shot('demo-08-approved');
    return { taskStatus: status, approval };
  });

  await step('D12 — منع الموافقة المكررة ومنع منح الاعتماد من مسار المنفّذ', async () => {
    const approved = withDb((db) => db.prepare("SELECT id FROM approvals WHERE resource_type='TASK' AND decision='APPROVED' ORDER BY rowid DESC LIMIT 1").get());
    const duplicate = await ui.ipc('decide_approval', {
      input: { approvalId: approved.id, decision: 'APPROVED', reason: 'duplicate approval attempt' },
    });
    assert(duplicate.ok === false, 'duplicate approval unexpectedly succeeded');
    const directApprove = await ui.ipc('transition_task', {
      input: { taskId: taskId(), targetStatus: 'APPROVED', reason: 'direct approval attempt', expectedVersion: scalar('SELECT version FROM tasks WHERE id = ?', taskId()) },
    });
    assert(directApprove.ok === false, 'direct APPROVED transition must be refused from the performer path');
    const statusAfter = scalar('SELECT status FROM tasks WHERE id = ?', taskId());
    assert(statusAfter === 'APPROVED', `task status must stay APPROVED, got ${statusAfter}`);
    return { duplicate, directApprove, taskStatus: statusAfter };
  });

  await step('D13 — إغلاق المهمة المعتمدة ثم حماية الحالة النهائية', async () => {
    await ui.selectActor('user_demo_operator');
    await ui.nav(AR.navTasks);
    await ui.clickButton(AR.transitionClose);
    await ui.waitDialog();
    await ui.applyExpectSuccess();
    await ui.waitRowStatus(AR.statusCompleted);
    const row = await ui.taskRow();
    const completedText = (await row.textContent()) ?? '';
    const terminal = await ui.ipc('transition_task', {
      input: { taskId: taskId(), targetStatus: 'IN_PROGRESS', reason: 'reopen attempt', expectedVersion: scalar('SELECT version FROM tasks WHERE id = ?', taskId()) },
    });
    assert(terminal.ok === false, 'completed task must be terminal');
    assert(completedText.includes(AR.statusCompleted), 'completed status not shown');
    await shot('demo-09-completed');
    return { taskTerminal: true, terminalAttempt: terminal };
  });

  await step('D14 — سجل الانتقالات الكامل يطابق الرحلة المطلوبة', async () => {
    const transitions = rows('SELECT from_status, to_status, reason FROM task_state_transitions ORDER BY created_at ASC, rowid ASC');
    report.db.transitions = transitions;
    const sequence = transitions.map((item) => `${item.from_status}->${item.to_status}`);
    // الرحلة الكاملة المتوقعة: بدء ← مراجعة ← رفض ← إعادة عمل ← مراجعة ← اعتماد ← إغلاق.
    const expected = [
      'OPEN->IN_PROGRESS',
      'IN_PROGRESS->READY_FOR_REVIEW',
      'READY_FOR_REVIEW->REJECTED',
      'REJECTED->IN_PROGRESS',
      'IN_PROGRESS->READY_FOR_REVIEW',
      'READY_FOR_REVIEW->APPROVED',
      'APPROVED->COMPLETED',
    ];
    assert(sequence.length === expected.length, `expected ${expected.length} transitions, got ${sequence.length}: ${sequence.join(',')}`);
    for (let index = 0; index < expected.length; index += 1) {
      assert(sequence[index] === expected[index], `transition ${index} mismatch: got ${sequence[index]} expected ${expected[index]}`);
    }
    return { count: sequence.length, sequence };
  });

  await step('D15 — اللوحة التنفيذية والمؤشرات تطابق الصفوف الأصلية في قاعدة الاختبار', async () => {
    const db = {
      total: scalar('SELECT COUNT(*) FROM tasks'),
      open: scalar("SELECT COUNT(*) FROM tasks WHERE status='OPEN'"),
      inProgress: scalar("SELECT COUNT(*) FROM tasks WHERE status='IN_PROGRESS'"),
      ready: scalar("SELECT COUNT(*) FROM tasks WHERE status='READY_FOR_REVIEW'"),
      rejected: scalar("SELECT COUNT(*) FROM tasks WHERE status='REJECTED'"),
      completed: scalar("SELECT COUNT(*) FROM tasks WHERE status='COMPLETED'"),
      overdue: scalar("SELECT COUNT(*) FROM tasks WHERE due_at IS NOT NULL AND due_at < ? AND status <> 'COMPLETED'", new Date().toISOString()),
      pendingApprovals: scalar("SELECT COUNT(*) FROM approvals WHERE decision='PENDING'"),
      evidence: scalar('SELECT COUNT(*) FROM evidence_items'),
      obligations: scalar('SELECT COUNT(*) FROM obligations'),
    };
    await ui.nav(AR.navTasks);
    const cards = await page.locator('#tasks .metric-card').evaluateAll((nodes) =>
      nodes.map((node) => ({ label: (node.querySelector('span')?.textContent ?? '').trim(), value: (node.querySelector('strong')?.textContent ?? '').trim() })));
    const readCard = (fragment) => {
      const card = cards.find((item) => item.label.includes(fragment));
      return card ? Number(card.value) : null;
    };
    const uiValues = {
      total: readCard('إجمالي المهام'),
      open: readCard('المهام المفتوحة'),
      inProgress: readCard('المهام قيد التنفيذ'),
      ready: readCard('بانتظار المراجعة'),
      rejected: readCard('مرفوضة'),
      completed: readCard('المهام المكتملة'),
      overdue: readCard('متأخرة'),
    };
    await ui.nav(AR.navOverview);
    const overview = await page.evaluate(() => document.querySelector('main')?.innerText ?? '');
    const chainValid = overview.includes(AR.chainValid);
    const readinessNotMeasured = overview.includes(AR.readinessNotMeasured);
    // حساب الجاهزية نفسه من الصفوف الأصلية — نفس صيغة الخلف المُعلنة.
    const eligible = rows("SELECT id, severity, status, verification_state FROM obligations WHERE applicability = 'APPLIES'");
    const covered = new Set(rows("SELECT DISTINCT obligation_id FROM evidence_items WHERE status IN ('UPLOADED','PROCESSING','ANALYZED','VALID')").map((item) => item.obligation_id));
    const humanVerified = eligible.filter((item) => item.verification_state === 'HUMAN_VERIFIED').length;
    const evidenceCovered = eligible.filter((item) => covered.has(item.id)).length;
    const eligibleCount = eligible.length;
    const coverage = eligibleCount ? Math.floor((humanVerified * 100) / eligibleCount) : null;
    const completeness = eligibleCount ? Math.floor((evidenceCovered * 100) / eligibleCount) : null;
    const score = coverage === null ? null : Math.floor((coverage * 60 + completeness * 40) / 100);
    const openCritical = eligible.filter((item) => item.severity === 'CRITICAL' && item.status !== 'COMPLETED').length;
    const readinessUi = await page.locator('section[aria-label="تقدير الجاهزية"] .metric-card').evaluateAll((nodes) => nodes.map((node) => ({
      heading: (node.querySelector('.metric-card__heading span')?.textContent ?? node.querySelector('span')?.textContent ?? '').trim(),
      value: (node.querySelector('.readiness-ring span')?.textContent ?? node.querySelector('strong')?.textContent ?? '').trim(),
    })));
    const reportedCoverage = readinessUi[0]?.value ?? '';
    report.db.dashboardReadiness = { db: { eligibleCount, humanVerified, evidenceCovered, coverage, completeness, score, openCritical }, ui: readinessUi, reportedCoverage };
    if (eligibleCount === 0) {
      assert(reportedCoverage.includes(AR.readinessNotMeasured) || reportedCoverage === '—', `no eligible obligations must yield "not measured", got "${reportedCoverage}"`);
    } else {
      assert(reportedCoverage === (score === null ? '—' : `${score}%`), `readiness score mismatch: UI="${reportedCoverage}" expected="${score}%"`);
      assert(readinessUi[1]?.value === `${coverage}%`, `verified coverage mismatch: UI="${readinessUi[1]?.value}" expected="${coverage}%"`);
      assert(readinessUi[2]?.value === `${completeness}%`, `evidence completeness mismatch: UI="${readinessUi[2]?.value}" expected="${completeness}%"`);
      assert(readinessUi[3]?.value === String(openCritical), `critical items mismatch: UI="${readinessUi[3]?.value}" expected="${openCritical}"`);
    }
    report.db.dashboard = { db, uiValues, chainValid, readinessNotMeasured, readiness: report.db.dashboardReadiness };
    for (const key of ['total', 'open', 'inProgress', 'ready', 'rejected', 'completed', 'overdue']) {
      assert(uiValues[key] === db[key], `metric ${key}: UI=${uiValues[key]} DB=${db[key]}`);
    }
    assert(chainValid, 'dashboard reports a broken audit chain after the full lifecycle');
    await shot('demo-10-dashboard');
    return { db, uiValues, chainValid, readinessNotMeasured, pendingApprovals: db.pendingApprovals };
  });

  await step('D16 — التقارير تُبنى من سجلات حقيقية وتطابق عدّ القاعدة', async () => {
    await ui.nav(AR.navReports);
    await page.waitForSelector('#reports-catalog-title', { timeout: 20000 });
    const catalogCount = await page.locator('.report-catalog__item').count();
    await page.locator('.report-catalog__item', { hasText: 'عبء المهام' }).first().click();
    await page.waitForSelector('#reports-result-title', { timeout: 20000 });
    await sleep(1500);
    const body = await page.evaluate(() => document.querySelector('main')?.innerText ?? '');
    const generatedAt = body.includes('لحظة التوليد');
    const hasProvenance = body.includes('الجداول المصدر');
    const disclaimer = body.includes('ليس إقراراً بالامتثال القانوني');
    assert(catalogCount === 6, `report catalogue must expose the 6 frozen definitions, got ${catalogCount}`);
    assert(generatedAt && hasProvenance, 'report provenance block missing');
    assert(disclaimer, 'report must state it is not a legal compliance certification');
    assert(body.includes('NEXUS Demo Operator'), 'workload report does not show the real assignee from the database');
    // مطابقة أرقام التقرير مع عدّ الصفوف الحقيقي في قاعدة الاختبار عبر أمر الخلف نفسه.
    const viaIpc = await ui.ipc('generate_report', { input: { reportId: 'TASK_WORKLOAD' } });
    assert(viaIpc.ok, `generate_report failed: ${JSON.stringify(viaIpc)}`);
    const sections = viaIpc.result.sections ?? [];
    const workloadSection = sections.find((section) => section.columns.some((column) => column.key === 'assigneeUserId') && section.columns.some((column) => column.key === 'totalTasks'));
    assert(workloadSection, 'TASK_WORKLOAD report has no assignee section');
    const columnIndex = (key) => workloadSection.columns.findIndex((column) => column.key === key);
    const workloadRow = workloadSection.rows.find((row) => row[columnIndex('assigneeUserId')] === 'user_demo_operator');
    const dbTasks = scalar('SELECT COUNT(*) FROM tasks');
    const dbCompleted = scalar("SELECT COUNT(*) FROM tasks WHERE status = 'COMPLETED'");
    assert(workloadRow, `report has no row for the real assignee; rows=${JSON.stringify(workloadSection.rows).slice(0, 300)}`);
    const reportTotal = Number(workloadRow[columnIndex('totalTasks')]);
    const reportCompleted = Number(workloadRow[columnIndex('completedTasks')]);
    assert(reportTotal === dbTasks, `report total ${reportTotal} != DB tasks ${dbTasks}`);
    assert(reportCompleted === dbCompleted, `report completed ${reportCompleted} != DB completed ${dbCompleted}`);
    report.db.report = { catalogCount, sections: sections.map((section) => ({ title: section.title, totalRows: section.totalRows })), workloadRow, generatedAt };
    await shot('demo-11-report');
    return { catalogCount, provenance: { generatedAt, hasProvenance, disclaimer }, reportTotal, reportCompleted, dbTasks, dbCompleted };
  });

  await step('D17 — تصدير التقرير عبر أمر الخلف الحقيقي (CSV وMarkdown) والتحقق من سلامة الملف', async () => {
    const outDir = EVIDENCE ? path.resolve(EVIDENCE) : path.resolve('.tmp-uat');
    const csvPath = path.join(outDir, 'uat-report.csv');
    const mdPath = path.join(outDir, 'uat-report.md');
    const csv = await ui.ipc('export_report', { input: { reportId: 'TASK_WORKLOAD', format: 'CSV', targetPath: csvPath } });
    const md = await ui.ipc('export_report', { input: { reportId: 'TASK_WORKLOAD', format: 'MARKDOWN', targetPath: mdPath } });
    assert(csv.ok, `CSV export failed: ${JSON.stringify(csv)}`);
    assert(md.ok, `Markdown export failed: ${JSON.stringify(md)}`);
    const { readFileSync } = await import('node:fs');
    const csvText = readFileSync(csvPath, 'utf8');
    const mdText = readFileSync(mdPath, 'utf8');
    assert(csvText.split(/\r?\n/).length >= 2, 'CSV file has no data rows');
    assert(mdText.length > 100, 'Markdown export too small');
    assert(mdText.includes('NEXUS') || mdText.includes('تقرير'), 'Markdown export missing provenance header');
    const exportEvent = scalar("SELECT COUNT(*) FROM audit_events WHERE event_type = 'REPORT_EXPORTED'");
    assert(exportEvent >= 2, `each export must append a REPORT_EXPORTED audit event, got ${exportEvent}`);
    report.db.reportExport = { csvBytes: csvText.length, mdBytes: mdText.length, exportAuditEvents: exportEvent, csvHead: csvText.split(/\r?\n/).slice(0, 3) };
    return report.db.reportExport;
  });

  await step('D17b — فصل السلطة: المراجِع لا يملك تصدير الملفات', async () => {
    await ui.selectActor('user_demo_reviewer');
    const attempt = await ui.ipc('export_report', { input: { reportId: 'TASK_WORKLOAD', format: 'CSV', targetPath: path.join(EVIDENCE ? path.resolve(EVIDENCE) : path.resolve('.tmp-uat'), 'reviewer-export.csv') } });
    const readOnly = await ui.ipc('generate_report', { input: { reportId: 'TASK_WORKLOAD' } });
    assert(attempt.ok === false, 'reviewer without report:export unexpectedly exported a file');
    assert(readOnly.ok === true, 'reviewer with report:read should still be able to view the report');
    await ui.selectActor('user_demo_operator');
    return { exportAttempt: attempt, readAllowed: readOnly.ok };
  });

  await step('D18 — سجل التدقيق يعرض الرحلة كاملة والسلسلة سليمة', async () => {
    const chain = auditChainCheck();
    const eventTypes = rows('SELECT event_type, COUNT(*) AS total FROM audit_events GROUP BY event_type ORDER BY event_type');
    report.db.demoAudit = { chain, eventTypes };
    assert(chain.brokenLinks === 0, `audit chain broken: ${chain.brokenLinks}`);
    // المهمة في الديمو مزروعة لا مُنشأة من الواجهة، فحدث TASK_CREATED لا يُتوقع؛
    // المطلوب هو كل أحداث الرحلة التي نفّذها المستخدم فعلاً.
    for (const type of ['TASK_STARTED', 'TASK_SUBMITTED_FOR_REVIEW', 'TASK_REJECTED', 'TASK_APPROVED', 'TASK_COMPLETED', 'EVIDENCE_REFERENCE_REGISTERED']) {
      const found = eventTypes.some((item) => item.event_type === type);
      assert(found, `audit event type missing: ${type}`);
    }
    await ui.nav(AR.navAudit);
    const auditText = await page.evaluate(() => document.querySelector('main')?.innerText ?? '');
    assert(auditText.length > 200, 'audit page rendered no events');
    await shot('demo-12-audit');
    return { chain, eventTypes };
  });

  await step('D19 — اختبار سلبي: صلاحيات إدارة المستخدمين مرفوضة في الخلف لمنفّذ العمل', async () => {
    const attempt = await ui.ipc('set_user_status', { input: { userId: 'user_demo_reviewer', status: 'DISABLED' } });
    const stillActive = scalar("SELECT status FROM local_users WHERE id = 'user_demo_reviewer'");
    assert(attempt.ok === false, 'unauthorized user administration unexpectedly succeeded');
    assert(stillActive === 'ACTIVE', `reviewer status changed to ${stillActive}`);
    return { attempt, reviewerStatus: stillActive };
  });

  await step('D20 — سلامة قاعدة الاختبار: قيود أجنبية وتكامل وسلسلة وعزل مستأجرين', async () => {
    const integrity = withDb((db) => db.prepare('PRAGMA integrity_check').get());
    const foreignKeyViolations = withDb((db) => db.prepare('PRAGMA foreign_key_check').all());
    const orphanApprovals = scalar("SELECT COUNT(*) FROM approvals a WHERE NOT EXISTS (SELECT 1 FROM tasks t WHERE t.id = a.resource_id AND t.tenant_id = a.tenant_id)");
    const crossTenantTasks = scalar("SELECT COUNT(*) FROM tasks t JOIN obligations o ON o.id = t.obligation_id WHERE o.tenant_id <> t.tenant_id");
    const chain = auditChainCheck();
    const snapshot = {
      integrity,
      foreignKeyViolations: foreignKeyViolations.length,
      orphanApprovals,
      crossTenantTasks,
      chain,
      tasks: scalar('SELECT COUNT(*) FROM tasks'),
      transitions: scalar('SELECT COUNT(*) FROM task_state_transitions'),
      approvals: scalar('SELECT COUNT(*) FROM approvals'),
      evidence: scalar('SELECT COUNT(*) FROM evidence_items'),
    };
    report.db.integrity = snapshot;
    assert(foreignKeyViolations.length === 0, `foreign key violations: ${JSON.stringify(foreignKeyViolations)}`);
    assert(orphanApprovals === 0, `orphan approvals: ${orphanApprovals}`);
    assert(crossTenantTasks === 0, `cross-tenant tasks: ${crossTenantTasks}`);
    assert(chain.brokenLinks === 0, 'audit chain broken');
    const integrityValue = Object.values(integrity ?? {})[0];
    assert(String(integrityValue) === 'ok', `integrity_check: ${integrityValue}`);
    return snapshot;
  });
};

// =====================================================================
// وصل نافذة التطبيق الفعلية عبر CDP + أدوات الرحلة المؤسسية
// =====================================================================

const connectToApp = async () => {
  const version = await waitForCdp(PORT);
  report.browser = { Browser: version.Browser };
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${PORT}`);
  const context = browser.contexts()[0] ?? (await browser.newContext());
  let found = context.pages().find((p) => !p.url().startsWith('devtools://'));
  const deadline = Date.now() + 45000;
  while (!found && Date.now() < deadline) {
    await sleep(300);
    found = context.pages().find((p) => !p.url().startsWith('devtools://'));
  }
  if (!found) throw new Error('no NEXUS application page found over CDP');
  page = found;
  report.pageUrl = page.url();
  page.on('console', (message) => {
    if (message.type() === 'error') report.consoleErrors.push(message.text().slice(0, 400));
  });
  page.on('pageerror', (error) => report.pageErrors.push(String(error?.message ?? error).slice(0, 400)));
  // تأكيدات الواجهة الأصلية (window.confirm) تُقبل تلقائياً: الجولة تقيس الرفض
  // الخلفي لا حوار المتصفح، ويجب ألا تعلّق الجولة على تأكيد يدوي.
  page.on('dialog', (dialog) => { void dialog.accept().catch(() => {}); });
  await page.waitForSelector('#root', { timeout: 60000 });
};

// إعادة تشغيل التطبيق فعلياً: إغلاق النافذة والعملية ثم فتح نافذة جديدة.
// الغرض إثبات أن البيانات والجلسات والتدقيق تبقى مع التطبيق نفسه لا في ذاكرة الواجهة.
const restartApp = async () => {
  if (browser) await browser.close().catch(() => {});
  browser = null;
  page = null;
  try { child?.kill(); } catch { /* العملية انتهت */ }
  await sleep(3000);
  launchApp();
  await connectToApp();
  await page.waitForSelector('#setup-title, #login-email, nav.navigation', { timeout: 120000 });
};

const uiJourney = {
  async login(email, password) {
    await page.waitForSelector('#login-email', { timeout: 60000 });
    await page.fill('#login-email', email);
    await page.fill('#login-password', password);
    await page.getByRole('button', { name: AR.login, exact: true }).first().click();
    await page.waitForSelector('nav.navigation', { timeout: 60000 });
    await sleep(700);
  },
  async logout() {
    const button = page.getByRole('button', { name: AR.logout, exact: true }).first();
    await button.waitFor({ state: 'visible', timeout: 30000 });
    await button.click();
    await page.waitForSelector('#login-email', { timeout: 60000 });
    await sleep(400);
  },
  // الانتقال إلى قسم إعدادات بعينه (القائمة الجانبية ثم تبويب القسم).
  async settingsSection(label) {
    await ui.nav(AR.navSettings);
    const tab = page.locator('.settings-nav__item', { hasText: label }).first();
    await tab.waitFor({ state: 'visible', timeout: 20000 });
    await tab.click();
    await sleep(500);
  },
  async fillByLabel(root, label, value) {
    const field = root.getByLabel(label, { exact: true }).first();
    await field.waitFor({ state: 'visible', timeout: 20000 });
    await field.fill(value);
    await sleep(120);
  },
  async selectByLabel(root, label, value) {
    const field = root.getByLabel(label, { exact: true }).first();
    await field.waitFor({ state: 'visible', timeout: 20000 });
    await field.selectOption(value);
    await sleep(120);
  },
  // نقر زر بالاسم الدقيق داخل نطاق محدّد (الحوار مثلاً) لتجنّب التباس الأسماء.
  async click(root, name) {
    const button = root.getByRole('button', { name, exact: true }).first();
    await button.waitFor({ state: 'visible', timeout: 20000 });
    await button.click();
    await sleep(350);
  },
  async waitStatusInTasks(title, label, timeout = 20000) {
    const deadline = Date.now() + timeout;
    let last = '';
    while (Date.now() < deadline) {
      const row = page.locator('#tasks table tbody tr', { hasText: title }).first();
      const count = await row.count();
      if (count > 0) {
        last = (await row.textContent()) ?? '';
        if (last.includes(label)) return last;
      }
      await page.getByRole('button', { name: AR.refresh, exact: false }).first().click().catch(() => {});
      await sleep(400);
    }
    throw new Error(`task "${title}" never showed status "${label}" — last: ${last.slice(0, 240)}`);
  },
};

// كشف حوار الحفظ الأصلي في Windows وإلغاؤه: إثبات أن مسار التصدير يفتح الحوار
// الحقيقي وأن الإلغاء لا يترك ملفاً. (اختيار وجهة وسائط فعلية غير قابل للأتمتة
// داخل WebView2، فيُوثَّق القيد بدل تزوير نجاح — انظر UAT-06.)
const nativeSaveDialog = {
  probe(cancel = true) {
    const script = [
      'Add-Type @"',
      'using System;',
      'using System.Text;',
      'using System.Runtime.InteropServices;',
      'public class NexusWin {',
      '  public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);',
      '  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr p);',
      '  [DllImport("user32.dll")] public static extern int GetClassName(IntPtr hWnd, StringBuilder sb, int n);',
      '  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);',
      '  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);',
      '  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd, uint msg, IntPtr w, IntPtr l);',
      '}',
      '"@',
      '$pids = @(Get-Process -Name nexus_desktop -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id)',
      '$script:found = 0',
      '$cb = [NexusWin+EnumProc]{',
      '  param($hWnd, $lParam)',
      '  if (-not [NexusWin]::IsWindowVisible($hWnd)) { return $true }',
      '  $sb = New-Object System.Text.StringBuilder 256',
      '  [void][NexusWin]::GetClassName($hWnd, $sb, 256)',
      '  if ($sb.ToString() -eq "#32770") {',
      '    $procId = 0',
      '    [void][NexusWin]::GetWindowThreadProcessId($hWnd, [ref]$procId)',
      '    if ($pids -contains $procId) {',
      '      $script:found = 1',
      '      if ($env:NEXUS_UAT_CANCEL_DIALOG -eq "1") { [void][NexusWin]::PostMessage($hWnd, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) }',
      '    }',
      '  }',
      '  return $true',
      '}',
      '[void][NexusWin]::EnumWindows($cb, [IntPtr]::Zero)',
      'Write-Output $script:found',
    ].join('\n');
    const env = { ...process.env };
    if (cancel) env.NEXUS_UAT_CANCEL_DIALOG = '1';
    const result = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script], {
      encoding: 'utf8',
      env,
      timeout: 30000,
    });
    if (result.error) return { detected: false, error: String(result.error) };
    return { detected: (result.stdout ?? '').trim().endsWith('1'), raw: (result.stdout ?? '').trim().slice(-200) };
  },
};
// ===== التشغيل =====
try {
  await connectToApp();

  if (PHASE === 'production') await runProductionPhase();
  else if (PHASE === 'demo') await runDemoPhase();
  else throw new Error(`unknown phase: ${PHASE}`);

  report.consoleErrorsFinal = report.consoleErrors.slice();
  report.pageErrorsFinal = report.pageErrors.slice();
  await step('Z1 — لا أخطاء صفحة ولا أخطاء console خلال الجولة', async () => {
    assert(report.pageErrors.length === 0, `page errors: ${report.pageErrors.join(' ; ')}`);
    assert(report.consoleErrors.length === 0, `console errors: ${report.consoleErrors.join(' ; ')}`);
    return { pageErrors: 0, consoleErrors: 0 };
  });
} catch (error) {
  report.fatal = String(error?.message ?? error);
  console.error(`UAT_FATAL: ${report.fatal}`);
} finally {
  report.finishedAt = new Date().toISOString();
  saveReport();
  if (browser) await browser.close().catch(() => {});
  try { child.kill(); } catch { /* العملية انتهت */ }
  console.log(`\nUAT_SUMMARY phase=${PHASE} total=${report.summary.total} passed=${report.summary.passed} failed=${report.summary.failed} skipped=${report.summary.skipped}`);
  process.exit(report.summary.failed === 0 && !report.fatal ? 0 : 1);
}
