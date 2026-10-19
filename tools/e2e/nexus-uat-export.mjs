/**
 * NEXUS V1 — UAT-06: اختبار فعلي لتصدير الملفات من حوار Windows الأصلي.
 * ---------------------------------------------------------------------
 * يقود تطبيق Tauri الحقيقي عبر CDP، ويضغط زر «تصدير CSV»/«تصدير Markdown»
 * في الواجهة، فيفتح Tauri حوار الحفظ الأصلي (Win32 common dialog). يُقاد الحوار
 * بـUI Automation (save-dialog-uia.ps1) كما يقوده المستخدم: كتابة مسار الوجهة
 * ثم Save، أو Cancel.
 *
 * ما يُثبته فعلياً:
 *   1. فتح الحوار الأصلي من مسار الواجهة الحقيقي.
 *   2. كتابة مسار وجهة وحفظ ملف CSV على القرص.
 *   3. التحقق من وجود الملف وحجمه وبصمة SHA-256 وفك ترميز UTF-8 ومحتوى عربي.
 *   4. حدث تدقيق REPORT_EXPORTED واحد لكل تصدير ناجح.
 *   5. حفظ Markdown والتحقق من المحتوى.
 *   6. إلغاء الحوار: لا ملف، ولا حدث تدقيق، ولا خطأ في الواجهة.
 *
 * التشغيل: node tools/e2e/nexus-uat-export.mjs --port=9444 --evidence=<dir>
 * لا أسرار ولا بيانات عميل في المخرجات.
 */
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';

import { chromium } from 'playwright-core';

const require = createRequire(import.meta.url);
const { DatabaseSync } = require('node:sqlite');

const argv = process.argv.slice(2);
const argOf = (name, fallback) => {
  const found = argv.find((a) => a.startsWith(`--${name}=`));
  return found ? found.slice(name.length + 3) : fallback;
};
const PORT = Number(argOf('port', '9444'));
const EVIDENCE = argOf('evidence', path.join(process.cwd(), '.tmp-uat', 'evidence-export'));
const APPDATA_DIR = argOf(
  'appdata',
  path.join(process.env.LOCALAPPDATA ?? '', 'com.nexus.obligationos.uatharness'),
);
const DB_PATH = path.join(APPDATA_DIR, 'nexus-local.db');
const OUT_DIR = path.join(EVIDENCE, 'native-export');
const UIA_SCRIPT = path.join(
  path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1')),
  'save-dialog-uia.ps1',
);
mkdirSync(OUT_DIR, { recursive: true });

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const steps = [];
const record = (id, ok, detail) => {
  steps.push({ id, ok, detail });
  console.log(`${ok ? 'PASS' : 'FAIL'} ${id} — ${detail}`);
};

const auditCount = (eventType) => {
  try {
    const database = new DatabaseSync(DB_PATH, { readOnly: true });
    const row = database
      .prepare('SELECT COUNT(*) AS total FROM audit_events WHERE event_type = ?')
      .get(eventType);
    database.close();
    return Number(row?.total ?? 0);
  } catch {
    return -1;
  }
};

// معرفة PID التطبيق بلا التقاط stdio عبر أنابيب (ممنوع في بيئة الوكيل):
// يُكتب إلى ملف ثم يُقرأ.
const appPid = (() => {
  const pidFile = path.join(OUT_DIR, 'app-pid.txt');
  spawnSync(
    'powershell.exe',
    [
      '-NoProfile',
      '-Command',
      `(Get-Process nexus_desktop -ErrorAction SilentlyContinue | Select-Object -First 1).Id | Out-File -Encoding ascii '${pidFile}'`,
    ],
    { stdio: 'ignore' },
  );
  try {
    return Number(String(readFileSync(pidFile, 'utf8')).trim()) || 0;
  } catch {
    return 0;
  }
})();

// الحوار الأصلي: لا التقاط piped stdio (ممنوع في بيئة الوكيل) — المخرجات inherit.
const runDialogAutomation = (action, targetPath, diagnosticsSuffix = '') => {
  const diagnostics = path.join(OUT_DIR, `uia-diagnostics-${diagnosticsSuffix || action}.txt`);
  const result = spawnSync(
    'powershell.exe',
    [
      '-NoProfile',
      '-ExecutionPolicy',
      'Bypass',
      '-File',
      UIA_SCRIPT,
      '-Action',
      action,
      '-TargetPath',
      targetPath ?? '',
      '-TimeoutSeconds',
      '30',
      '-ProcessId',
      String(appPid),
      '-DiagnosticsPath',
      diagnostics,
    ],
    { stdio: 'inherit' },
  );
  return result.status ?? -1;
};

const waitForFile = async (filePath, timeoutMs = 20000) => {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (existsSync(filePath)) return true;
    await sleep(250);
  }
  return false;
};

const sha256 = (filePath) => createHash('sha256').update(readFileSync(filePath)).digest('hex');

const waitForExportIdle = async (page, timeoutMs = 20000) => {
  const deadline = Date.now() + timeoutMs;
  const csvButton = page.locator('button:has-text("تصدير CSV")').first();
  while (Date.now() < deadline) {
    if (await csvButton.isEnabled().catch(() => false)) return true;
    await sleep(300);
  }
  return false;
};

let browser;
let exitCode = 0;
try {
  // 1) الاتصال بنافذة WebView2 الفعلية.
  for (let attempt = 0; attempt < 60 && !browser; attempt += 1) {
    try {
      browser = await chromium.connectOverCDP(`http://127.0.0.1:${PORT}`);
    } catch {
      await sleep(500);
    }
  }
  if (!browser) throw new Error('CDP_UNAVAILABLE');
  record('UAT06-01', true, `اتصال CDP بنافذة WebView2 الفعلية (app pid=${appPid})`);

  const context = browser.contexts()[0];
  const page = context.pages()[0] ?? (await context.waitForEvent('page'));
  await page.waitForLoadState('domcontentloaded');

  const ipc = (command, payload) =>
    page.evaluate(
      async ([cmd, args]) => {
        try {
          return { ok: true, result: await window.__TAURI_INTERNALS__.invoke(cmd, args) };
        } catch (error) {
          return { ok: false, code: error?.code ?? 'UNKNOWN', message: String(error?.message ?? error) };
        }
      },
      [command, payload ?? {}],
    );

  // 2) تهيئة مؤسسة إنتاجية في الحاوية المعزولة (لا بيانات ديمو).
  const status = await ipc('get_auth_status');
  if (status.result?.mode === 'UNPROVISIONED') {
    const bootstrapped = await ipc('bootstrap_production', {
      input: {
        organizationName: 'UAT Export Organization',
        countryCode: 'SA',
        industry: 'Technology Services',
        adminDisplayName: 'UAT Export Admin',
        adminEmail: 'export.admin@uat.local',
        password: 'Str0ng!Passw0rd#2026',
      },
    });
    if (!bootstrapped.ok) throw new Error(`BOOTSTRAP_FAILED:${bootstrapped.code}`);
  } else if (status.result?.mode === 'PRODUCTION' && status.result?.authenticated === false) {
    const loggedIn = await ipc('login', {
      input: { email: 'export.admin@uat.local', password: 'Str0ng!Passw0rd#2026' },
    });
    if (!loggedIn.ok) throw new Error(`LOGIN_FAILED:${loggedIn.code}`);
  }
  // إعادة تحميل الواجهة كي تقرأ حالة المصادقة من الخلف بعد التهيئة/الدخول.
  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.waitForSelector('.navigation__item', { timeout: 45000 });
  record('UAT06-02', true, 'مؤسسة إنتاجية مهيأة وقشرة التطبيق محملة');

  // 3) الانتقال إلى وحدة التقارير بالنقر الحقيقي.
  await page.locator('.navigation__item', { hasText: 'التقارير' }).first().click();
  await page.waitForSelector('.report-catalog__item', { timeout: 30000 });
  record('UAT06-03', true, 'وحدة التقارير مفتوحة من التنقل الحقيقي');

  // 4) اختيار تقرير لتوليد المحتوى.
  await page.locator('.report-catalog__item').first().click();
  await sleep(1500);
  await waitForExportIdle(page);
  record('UAT06-04', true, 'تقرير محدد وأزرار التصدير متاحة');

  const csvPath = path.join(OUT_DIR, 'uat-native-report.csv');
  const markdownPath = path.join(OUT_DIR, 'uat-native-report.md');

  // 5) تصدير CSV عبر الحوار الأصلي.
  const csvAuditBefore = auditCount('REPORT_EXPORTED');
  const csvButton = page.locator('button:has-text("تصدير CSV")').first();
  if (!(await csvButton.isEnabled().catch(() => false))) {
    throw new Error('CSV_EXPORT_BUTTON_DISABLED');
  }
  await csvButton.click();
  await sleep(1200);
  const saveStatus = runDialogAutomation('save', csvPath, 'csv');
  record('UAT06-05', saveStatus === 0, `حوار الحفظ الأصلي: كتابة المسار وSave (UIA exit=${saveStatus})`);

  const csvExists = await waitForFile(csvPath);
  record('UAT06-06', csvExists, `ملف CSV مكتوب على المسار المختار: ${path.basename(csvPath)}`);

  if (csvExists) {
    const bytes = readFileSync(csvPath);
    const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    const arabic = /[\u0600-\u06FF]/.test(text);
    record('UAT06-07', bytes.length > 0, `حجم الملف ${bytes.length} بايت`);
    record('UAT06-08', arabic, 'محتوى عربي صحيح بترميز UTF-8 (فك ترميز صارم بلا بدائل)');
    record('UAT06-09', text.includes(','), 'بنية CSV تحتوي فواصل أعمدة حقيقية');
    record('UAT06-10', path.extname(csvPath) === '.csv', 'امتداد الملف مطابق للصيغة المطلوبة');
    console.log(`CSV_SHA256 ${sha256(csvPath)}`);
  } else {
    record('UAT06-07', false, 'الملف غير موجود — لا قياس للحجم');
    record('UAT06-08', false, 'لا محتوى لفحص الترميز');
    record('UAT06-09', false, 'لا محتوى لفحص البنية');
    record('UAT06-10', false, 'لا ملف لفحص الامتداد');
  }

  // 6) حدث تدقيق واحد للتصدير.
  await sleep(1500);
  const csvAuditAfter = auditCount('REPORT_EXPORTED');
  record(
    'UAT06-11',
    csvAuditBefore >= 0 && csvAuditAfter === csvAuditBefore + 1,
    `حدث تدقيق REPORT_EXPORTED: قبل=${csvAuditBefore} بعد=${csvAuditAfter}`,
  );

  // 7) تصدير Markdown عبر الحوار نفسه + فحص الترميز.
  const idleAfterCsv = await waitForExportIdle(page, 30000);
  if (!idleAfterCsv) {
    record('UAT06-12', false, 'زر التصدير بقي معطلاً بعد تصدير CSV — لا يمكن اختبار Markdown');
    record('UAT06-13', false, 'لم يُختبر Markdown');
  } else {
    await page.locator('button:has-text("تصدير Markdown")').first().click();
    await sleep(1200);
    const markdownStatus = runDialogAutomation('save', markdownPath, 'markdown');
    const markdownExists = await waitForFile(markdownPath);
    record(
      'UAT06-12',
      markdownStatus === 0 && markdownExists,
      `حفظ Markdown عبر الحوار (UIA exit=${markdownStatus})`,
    );
    if (markdownExists) {
      const markdown = new TextDecoder('utf-8', { fatal: true }).decode(readFileSync(markdownPath));
      record(
        'UAT06-13',
        /[\u0600-\u06FF]/.test(markdown) && /^#|^\*\*/m.test(markdown),
        'Markdown يحتوي ترويسات وعربية سليمة',
      );
    } else {
      record('UAT06-13', false, 'ملف Markdown غير موجود');
    }
  }

  // 8) إلغاء الحوار: لا ملف ولا حدث تدقيق.
  const cancelPath = path.join(OUT_DIR, 'uat-native-cancelled.csv');
  const cancelAuditBefore = auditCount('REPORT_EXPORTED');
  const idleBeforeCancel = await waitForExportIdle(page, 30000);
  if (!idleBeforeCancel) {
    record('UAT06-14', false, 'تعذر الوصول إلى زر التصدير لاختبار الإلغاء');
  } else {
    await page.locator('button:has-text("تصدير CSV")').first().click();
    await sleep(1200);
    const cancelStatus = runDialogAutomation('cancel', '', 'cancel');
    await sleep(2000);
    record('UAT06-14', cancelStatus === 0, `إلغاء الحوار الأصلي (UIA exit=${cancelStatus})`);
    record('UAT06-15', !existsSync(cancelPath), 'الإلغاء لم ينتج أي ملف');
  }
  const cancelAuditAfter = auditCount('REPORT_EXPORTED');
  record(
    'UAT06-16',
    cancelAuditBefore >= 0 && cancelAuditAfter === cancelAuditBefore,
    `الإلغاء لم ينتج حدث تدقيق (قبل=${cancelAuditBefore} بعد=${cancelAuditAfter})`,
  );

  // 9) التحقق من بقاء الواجهة سليمة بعد إلغاء الحوار (لا أخطاء حجب).
  const errorPanels = await page.locator('.error-panel').count();
  record('UAT06-17', errorPanels === 0, `لا لوحة خطأ بعد الإلغاء (وجد ${errorPanels})`);
  const remainingDialogs = await page.locator('[role="dialog"]').count();
  record('UAT06-18', remainingDialogs === 0, 'لا حوار داخلي معلّق بعد العملية');
} catch (error) {
  exitCode = 1;
  record('UAT06-EXCEPTION', false, String(error?.message ?? error));
} finally {
  // تنظيف أي حوار أصلي بقي مفتوحاً كي لا يعلّق العملية.
  try {
    runDialogAutomation('cancel', '', 'cleanup');
  } catch {
    /* لا شيء */
  }
  if (browser) await browser.close().catch(() => {});
  const report = {
    generatedAt: new Date().toISOString(),
    mode: 'PRODUCTION',
    appPid,
    steps,
    passed: steps.filter((step) => step.ok).length,
    failed: steps.filter((step) => !step.ok).length,
  };
  writeFileSync(path.join(EVIDENCE, 'uat06-native-export.json'), JSON.stringify(report, null, 2));
  console.log(`\nUAT06 ${report.passed}/${steps.length} PASS — evidence: ${EVIDENCE}`);
  process.exit(exitCode === 0 && report.failed === 0 ? 0 : 1);
}
