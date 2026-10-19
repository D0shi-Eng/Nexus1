// تحقق بصري حقيقي بمحرّك Chromium (Edge) على حزمة الإنتاج المبنية فعلاً.
//
// النطاق المعلن بصدق:
//   - هذا ليس E2E داخل WebView2. بُوابة CDP في WebView2 مثبتة الفشل في هذه البيئة
//     (E2E_ENVIRONMENT_LIMITED)، فلا يُدّعى أي تغطية لها.
//   - ما يُثبت هنا: أن حزمة `dist` الحقيقية تُحمَّل في محرّك Chromium حقيقي، وأن
//     رموز التصميم تُحلّ فعلياً، وأن الثيم الفاتح والداكن يغيّران القيم المحسوبة،
//     وأن RTL/LTR يتبعان اللغة، مع لقطات شاشة للوضعين وباللغتين.
//
// التشغيل: node tools/visual/nexus-visual-verify.mjs
import { createServer } from 'node:http';
import { readFile, mkdir } from 'node:fs/promises';
import { extname, join, normalize } from 'node:path';
import { chromium } from 'playwright-core';

const DIST = 'D:/NEXUS/apps/desktop/dist';
const OUT = 'D:/NEXUS/audit/visual-verification';
/** الثيمات الستّة — مطابقة لـ`themeOrder` المولَّد في src/lib/theme-order.ts. */
const THEMES = ['light', 'dark', 'midnight', 'sand', 'emerald', 'contrast'];
const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.ico': 'image/x-icon',
  '.woff2': 'font/woff2',
};

const startServer = async () => {
  const server = createServer(async (request, response) => {
    const url = new URL(request.url, 'http://127.0.0.1');
    // طلب أيقونة المتصفح الافتراضي ليس مورداً في التطبيق (Tauri لا يطلبه)؛ نُجيب بلا محتوى.
    if (url.pathname === '/favicon.ico') {
      response.writeHead(204);
      response.end();
      return;
    }
    const relative = normalize(decodeURIComponent(url.pathname)).replace(/^[\\/]+/, '');
    const target = join(DIST, relative === '' ? 'index.html' : relative);
    try {
      const body = await readFile(target);
      response.writeHead(200, { 'content-type': MIME[extname(target)] ?? 'application/octet-stream' });
      response.end(body);
    } catch {
      response.writeHead(404);
      response.end('not found');
    }
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  return { server, port: server.address().port };
};

// جسر IPC مزيّف: يُعيد بيانات عرضية فقط. لا يُزوَّر أي نجاح خلفي —
// كل أمر غير مذكور هنا يفشل صراحةً فتُظهر الواجهة حالاتها الحقيقية الآمنة.
const IPC_STUB = `
window.__NEXUS_INVOKED__ = [];
window.__TAURI_INTERNALS__ = {
  metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
  transformCallback: function (callback) { return callback; },
  convertFileSrc: function (path) { return path; },
  invoke: function (command, args) {
    window.__NEXUS_INVOKED__.push({ command: command, args: args === undefined ? null : args });
    var now = '2026-10-15T12:00:00Z';
    var data = {
      get_auth_status: { mode: 'DEMO', authenticated: true, needsBootstrap: false, actorDisplayName: 'Visual Harness Operator', sessionExpiresAt: null },
      get_session_context: { actorId: 'visual-harness', displayName: 'Visual Harness Operator', responsibility: 'REQUESTER', permissions: ['obligation:read', 'task:create', 'evidence:create', 'evidence:read', 'control:read', 'user:read', 'backup:create'] },
      get_health: { application: 'HEALTHY', database: 'HEALTHY', ai: 'NOT_CONFIGURED', checkedAt: now, appVersion: '0.1.0', schemaVersion: 0, runtimeMode: 'DEMO', configuredModelId: null, recentFailures: { agentRunsFailed24h: 0, providerErrors24h: 0 } },
      get_module_availability: [],
      get_demo_actors: { actors: [], activeActorId: 'visual-harness' },
      // GAP-05-OPEN: كتالوج دول شاشة التأسيس كما يصل من الخلف (قراءة عرضية فقط).
      get_bootstrap_options: { countryCodes: ['SA', 'AE', 'QA', 'KW', 'BH', 'OM', 'EG', 'JO', 'GB', 'US'] }
    };
    if (Object.prototype.hasOwnProperty.call(data, command)) return Promise.resolve(data[command]);
    // الإقلاع لا يُزوَّر نجاحه إطلاقاً: يُرفض برمز آمن حقيقي بعد تسجيل الحمولة.
    if (command === 'bootstrap_production') {
      return Promise.reject({ code: 'BOOTSTRAP_INDUSTRY_INVALID', message: 'The industry name does not meet the input rules.' });
    }
    return Promise.reject('LOCAL_SERVICE_UNAVAILABLE');
  }
};
`;

// نفس الجسر لكن في حالة قاعدة غير مهيأة: شاشة التأسيس الأولى هي المعروضة.
const IPC_STUB_UNPROVISIONED = IPC_STUB.replace(
  "get_auth_status: { mode: 'DEMO', authenticated: true, needsBootstrap: false, actorDisplayName: 'Visual Harness Operator', sessionExpiresAt: null }",
  "get_auth_status: { mode: 'UNPROVISIONED', authenticated: false, needsBootstrap: true, actorDisplayName: null, sessionExpiresAt: null }",
);

const readTokens = () => {
  const styles = getComputedStyle(document.documentElement);
  const names = ['--border', '--surface-card', '--surface-page', '--text-strong', '--accent', '--status-success', '--font-sans', '--fs-body', '--space-4'];
  const out = {};
  for (const name of names) out[name] = styles.getPropertyValue(name).trim();
  const dismiss = document.querySelector('.sidebar__dismiss');
  const menu = document.querySelector('.mobile-menu');
  return {
    tokens: out,
    theme: document.documentElement.getAttribute('data-theme'),
    dir: document.documentElement.dir,
    lang: document.documentElement.lang,
    bodyFont: getComputedStyle(document.body).fontFamily,
    // إثبات أن الخط المضمَّن مُحمَّل فعلاً لا مجرد مذكور في الكومة.
    bundledFontLoaded: typeof document.fonts?.check === 'function'
      ? document.fonts.check('16px "IBM Plex Sans Arabic"')
      : null,
    loadedFontFaces: typeof document.fonts?.forEach === 'function'
      ? (function () {
        var families = [];
        document.fonts.forEach(function (face) { if (face.status === 'loaded') families.push(face.family); });
        return Array.from(new Set(families));
      })()
      : [],
    bodyBackground: getComputedStyle(document.body).backgroundColor,
    bodyColor: getComputedStyle(document.body).color,
    // عقد الاستجابة: زر إغلاق الشريط يظهر على الشاشات الضيقة فقط، وزر القائمة العكس.
    dismissDisplay: dismiss ? getComputedStyle(dismiss).display : 'absent',
    mobileMenuDisplay: menu ? getComputedStyle(menu).display : 'absent',
    panelBorder: (function () {
      var panel = document.querySelector('.panel, .error-panel, .auth-card');
      return panel ? getComputedStyle(panel).borderTopColor : null;
    })(),
  };
};

const run = async () => {
  await mkdir(OUT, { recursive: true });
  const { server, port } = await startServer();
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  const results = [];

  for (const language of ['ar', 'en']) {
    for (const theme of ['light', 'dark']) {
      const context = await browser.newContext({ viewport: { width: 1360, height: 880 }, deviceScaleFactor: 1 });
      await context.addInitScript(IPC_STUB);
      await context.addInitScript(`window.localStorage.setItem('nexus.theme', ${JSON.stringify(theme)});`);
      await context.addInitScript(`window.localStorage.setItem('i18nextLng', ${JSON.stringify(language)});`);
      const page = await context.newPage();
      const consoleErrors = [];
      const failedRequests = [];
      const fontResponses = [];
      page.on('console', (message) => { if (message.type() === 'error') consoleErrors.push(message.text().slice(0, 200)); });
      page.on('pageerror', (error) => consoleErrors.push(`pageerror: ${String(error).slice(0, 200)}`));
      // أي مورد يفشل تحميله (خط أو نمط أو سكربت) عيب حقيقي في الحزمة.
      page.on('response', (response) => {
        if (response.status() >= 400) failedRequests.push(`${response.status()} ${response.url()}`);
        // الإثبات القاطع: نزول ملف خط من حزمة التطبيق نفسها لا من خطوط النظام.
        if (response.url().includes('/fonts/')) fontResponses.push(`${response.status()} ${response.url().split('/').pop()}`);
      });

      await page.goto(`http://127.0.0.1:${port}/index.html`, { waitUntil: 'load' });
      // ننتظر جهوزية الخطوط قبل قياس الكومة المحسوبة.
      await page.evaluate(() => document.fonts.ready);
      await page.waitForTimeout(1200);
      const appErrors = consoleErrors.filter((text) => !/favicon/i.test(text) && !/Failed to load resource/i.test(text));

      const snapshot = await page.evaluate(readTokens);
      const appliedNode = await page.evaluate(() => document.documentElement.getAttribute('data-theme'));
      const screenshot = join(OUT, `nexus-${language}-${theme}.png`);
      await page.screenshot({ path: screenshot, fullPage: false });

      results.push({ language, theme, requestedTheme: theme, appliedTheme: appliedNode, screenshot, consoleErrors: appErrors, failedRequests, fontResponses, ...snapshot });
      await context.close();
    }
  }

  // ===== مصفوفة الثيمات: كل ثيم يُطبَّق فعلاً ويُنتج بصمة ألوان مميّزة =====
  const themeMatrix = [];
  for (const theme of THEMES) {
    const context = await browser.newContext({ viewport: { width: 1360, height: 880 }, deviceScaleFactor: 1 });
    await context.addInitScript(IPC_STUB);
    await context.addInitScript(`window.localStorage.setItem('nexus.theme', ${JSON.stringify(theme)});`);
    await context.addInitScript("window.localStorage.setItem('i18nextLng', 'ar');");
    const page = await context.newPage();
    await page.goto(`http://127.0.0.1:${port}/index.html`, { waitUntil: 'load' });
    await page.evaluate(() => document.fonts.ready);
    await page.waitForTimeout(900);
    const snapshot = await page.evaluate(readTokens);
    const screenshot = join(OUT, `nexus-theme-${theme}.png`);
    await page.screenshot({ path: screenshot, fullPage: false });
    themeMatrix.push({ theme, appliedTheme: snapshot.theme, screenshot, ...snapshot });
    await context.close();
  }

  // ===== شاشة التأسيس الأولى (UNPROVISIONED): حقلا الدولة والقطاع =====
  // الإثبات: الحقلان يظهران فعلاً في محرّك Chromium على حزمة dist الحقيقية،
  // بلا قيمة افتراضية، والتحقق الأمامي يمنع الإرسال، والحمولة الواصلة للخلف
  // تحمل القيم التي أدخلها المستخدم حرفياً. نجاح الإقلاع نفسه غير مزوَّر هنا:
  // الجسر يرفض الأمر برمز آمن حقيقي بعد تسجيل الحمولة.
  const setupScenarios = [];
  for (const [language, theme] of [['ar', 'light'], ['en', 'dark']]) {
    const context = await browser.newContext({ viewport: { width: 1360, height: 880 }, deviceScaleFactor: 1 });
    await context.addInitScript(IPC_STUB_UNPROVISIONED);
    await context.addInitScript(`window.localStorage.setItem('nexus.theme', ${JSON.stringify(theme)});`);
    await context.addInitScript(`window.localStorage.setItem('i18nextLng', ${JSON.stringify(language)});`);
    const page = await context.newPage();
    const consoleErrors = [];
    page.on('console', (message) => { if (message.type() === 'error') consoleErrors.push(message.text().slice(0, 200)); });
    page.on('pageerror', (error) => consoleErrors.push(`pageerror: ${String(error).slice(0, 200)}`));

    await page.goto(`http://127.0.0.1:${port}/index.html`, { waitUntil: 'load' });
    await page.evaluate(() => document.fonts.ready);
    await page.waitForSelector('#setup-country-code');
    await page.waitForFunction(() => document.querySelectorAll('#setup-country-code option').length > 1);

    const initial = await page.evaluate(() => {
      const country = document.querySelector('#setup-country-code');
      const industry = document.querySelector('#setup-industry');
      const options = [...country.querySelectorAll('option')];
      return {
        countryValue: country.value,
        industryValue: industry.value,
        optionCount: options.length,
        firstOptionValue: options[0] ? options[0].value : null,
        codes: options.slice(1).map((option) => option.value).sort(),
        countryLabel: document.querySelector('label[for="setup-country-code"] span')?.textContent ?? '',
        industryLabel: document.querySelector('label[for="setup-industry"] span')?.textContent ?? '',
        suggestions: document.querySelectorAll('#setup-industry-suggestions option').length,
        dir: document.documentElement.dir,
        lang: document.documentElement.lang,
      };
    });
    const setupScreenshot = join(OUT, `nexus-setup-${language}-${theme}.png`);
    await page.screenshot({ path: setupScreenshot, fullPage: true });

    // التحقق الأمامي: نموذج فارغ لا يرسل أي طلب إقلاع.
    await page.click('button[type="submit"]');
    await page.waitForSelector('.form-error');
    const emptyValidation = await page.evaluate(() => ({
      message: document.querySelector('.form-error')?.textContent ?? '',
      invocations: window.__NEXUS_INVOKED__.filter((entry) => entry.command === 'bootstrap_production').length,
    }));

    // إدخال بيانات حقيقية ثم إرسال: الحمولة تُسجَّل، والخلف يرفض (لا نجاح مزوَّر).
    await page.fill('#setup-organization-name', 'Harness Organization');
    await page.selectOption('#setup-country-code', 'QA');
    await page.fill('#setup-industry', 'Financial Services');
    await page.fill('#setup-admin-display-name', 'Harness Admin');
    await page.fill('#setup-admin-email', 'harness@example.test');
    await page.fill('#setup-password', 'Str0ng!Passw0rd#2026');
    await page.click('button[type="submit"]');
    await page.waitForFunction(
      () => window.__NEXUS_INVOKED__.filter((entry) => entry.command === 'bootstrap_production').length > 0,
    );
    await page.waitForTimeout(500);

    const submitted = await page.evaluate(() => {
      const invoked = window.__NEXUS_INVOKED__.filter((entry) => entry.command === 'bootstrap_production');
      const industry = document.querySelector('#setup-industry');
      const last = invoked[invoked.length - 1];
      return {
        invocationCount: invoked.length,
        payload: last && last.args && last.args.input ? last.args.input : null,
        errorMessage: document.querySelector('.form-error')?.textContent ?? '',
        industryAriaInvalid: industry ? industry.getAttribute('aria-invalid') : null,
        focusedId: document.activeElement ? document.activeElement.id : null,
      };
    });
    const submittedScreenshot = join(OUT, `nexus-setup-submitted-${language}-${theme}.png`);
    await page.screenshot({ path: submittedScreenshot, fullPage: true });

    setupScenarios.push({
      language,
      theme,
      initial,
      emptyValidation,
      submitted,
      setupScreenshot,
      submittedScreenshot,
      consoleErrors,
    });
    await context.close();
  }

  await browser.close();
  server.close();

  console.log(JSON.stringify({ scenarios: results, themeMatrix: themeMatrix.map((t) => ({ theme: t.theme, applied: t.appliedTheme, tokens: t.tokens })), setupScenarios }, null, 2));
  const light = results.find((row) => row.language === 'ar' && row.theme === 'light');
  const dark = results.find((row) => row.language === 'ar' && row.theme === 'dark');
  const checks = [
    ['رموز الحدود محلولة', Boolean(light.tokens['--border'])],
    ['رموز الأسطح محلولة', Boolean(light.tokens['--surface-card'])],
    ['الثيم الداكن يغيّر سطح الصفحة', light.tokens['--surface-page'] !== dark.tokens['--surface-page']],
    ['الثيم الداكن يغيّر لون النص', light.tokens['--text-strong'] !== dark.tokens['--text-strong']],
    ['الثيم الداكن يغيّر الحدود', light.tokens['--border'] !== dark.tokens['--border']],
    ['خط الواجهة من طبقة الرموز', light.bodyFont.includes('IBM Plex Sans Arabic') || light.bodyFont.includes('Segoe UI')],
    ['العربية تُنتج RTL', results.find((r) => r.language === 'ar').dir === 'rtl'],
    ['الإنجليزية تُنتج LTR', results.find((r) => r.language === 'en').dir === 'ltr'],
    ['الثيم المطلوب مطبَّق فعلاً', light.appliedTheme === 'light' && dark.appliedTheme === 'dark'],
    // عيب مؤكَّد أُصلح (CQ-06): زر إغلاق الشريط الجانبي كان ظاهراً على سطح المكتب.
    ['زر إغلاق الشريط مخفي على سطح المكتب', light.dismissDisplay === 'none'],
    ['زر القائمة مخفي على سطح المكتب', light.mobileMenuDisplay === 'none'],
    ['لا أخطاء في وحدة التحكّم', results.every((row) => row.consoleErrors.length === 0)],
    // إصلاح الهوية البصرية: الخط المرخّص المضمَّن يُحمَّل فعلاً بدل السقوط على بديل النظام.
    ['الخط المضمَّن IBM Plex Sans Arabic مُحمَّل فعلاً', light.bundledFontLoaded === true],
    // الإثبات القاطع: ملفا الخط العربي واللاتيني نزلا فعلاً من الحزمة (200).
    ['نزل ملف الخط العربي المضمَّن من الحزمة', light.fontResponses.some((entry) => entry.startsWith('200') && entry.includes('arabic'))],
    ['نزل ملف الخط اللاتيني المضمَّن من الحزمة', light.fontResponses.some((entry) => entry.startsWith('200') && entry.includes('latin'))],
    ['لا مورد فاشل في أي سيناريو', results.every((row) => row.failedRequests.length === 0)],
    // الثيمات الستّة: كل واحد يُطبَّق، وكلها ذات بصمة ألوان مميّزة (لا ثيم مكرّر).
    ['الثيمات الستّة مطبَّقة فعلاً', themeMatrix.every((row) => row.appliedTheme === row.theme)],
    [
      'لكل ثيم بصمة ألوان مميّزة',
      new Set(themeMatrix.map((row) => `${row.tokens['--surface-page']}|${row.tokens['--accent']}|${row.tokens['--text-strong']}`)).size === THEMES.length,
    ],
    [
      'كل ثيم يحلّ رموزه الأساسية',
      themeMatrix.every((row) => row.tokens['--border'] && row.tokens['--surface-card'] && row.tokens['--accent']),
    ],
  ];
  // ===== فحوص شاشة التأسيس الأولى (GAP-05-OPEN) =====
  const setupAr = setupScenarios.find((row) => row.language === 'ar');
  const setupEn = setupScenarios.find((row) => row.language === 'en');
  const expectedCatalog = 'AE,BH,EG,GB,JO,KW,OM,QA,SA,US';
  const payloadsIntact = [setupAr, setupEn].every((row) => {
    const payload = row.submitted.payload;
    return payload !== null
      && payload.organizationName === 'Harness Organization'
      && payload.countryCode === 'QA'
      && payload.industry === 'Financial Services'
      && payload.adminDisplayName === 'Harness Admin'
      && payload.adminEmail === 'harness@example.test'
      && payload.password === 'Str0ng!Passw0rd#2026';
  });
  // رفض الخلف ليس نجاحاً: الرسالة المعروضة يجب أن تكون رسالة الرفض المترجمة.
  const rejectionRendered = [setupAr, setupEn].every(
    (row) => row.submitted.errorMessage.length > 0
      && row.submitted.errorMessage !== row.emptyValidation.message,
  );
  const setupChecks = [
    ['شاشة التأسيس تعرض حقل الدولة بالعربية', setupAr.initial.countryLabel.includes('الدولة')],
    ['شاشة التأسيس تعرض حقل القطاع بالعربية', setupAr.initial.industryLabel.includes('القطاع')],
    ['شاشة التأسيس تعرض Country بالإنجليزية', setupEn.initial.countryLabel.includes('Country')],
    ['شاشة التأسيس تعرض Industry بالإنجليزية', setupEn.initial.industryLabel.includes('Industry')],
    ['لا دولة مفترضة قبل اختيار المستخدم', setupAr.initial.countryValue === '' && setupEn.initial.countryValue === ''],
    ['لا قطاع مفترض قبل كتابة المستخدم', setupAr.initial.industryValue === '' && setupEn.initial.industryValue === ''],
    ['أول خيار في قائمة الدول فارغ (لا اختيار مسبق)', setupAr.initial.firstOptionValue === '' && setupEn.initial.firstOptionValue === ''],
    ['قائمة الدول هي كتالوج الخلف نفسه', setupAr.initial.codes.join(',') === expectedCatalog && setupEn.initial.codes.join(',') === expectedCatalog],
    ['مقترحات القطاع متاحة كتلميح كتابة', setupAr.initial.suggestions >= 5 && setupEn.initial.suggestions >= 5],
    ['شاشة التأسيس عربية RTL', setupAr.initial.dir === 'rtl' && setupAr.initial.lang === 'ar'],
    ['شاشة التأسيس إنجليزية LTR', setupEn.initial.dir === 'ltr' && setupEn.initial.lang === 'en'],
    ['نموذج فارغ لا يرسل أي إقلاع للخلف', setupAr.emptyValidation.invocations === 0 && setupEn.emptyValidation.invocations === 0],
    ['رسالة التحقق العربية عربية فعلاً', /[\u0621-\u064A]/.test(setupAr.emptyValidation.message)],
    ['الحمولة تحمل القيم المُدخلة حرفياً', payloadsIntact],
    ['رفض الخلف يُعرض مترجماً ولا يُزوَّر نجاح', rejectionRendered],
    ['حقل القطاع مُعلّم ويعود إليه التركيز بعد الرفض', setupAr.submitted.industryAriaInvalid === 'true' && setupAr.submitted.focusedId === 'setup-industry'],
    ['طلب إقلاع واحد فقط لا تكرار', setupAr.submitted.invocationCount === 1 && setupEn.submitted.invocationCount === 1],
    ['لا أخطاء في وحدة التحكّم في شاشة التأسيس', setupScenarios.every((row) => row.consoleErrors.length === 0)],
  ];
  checks.push(...setupChecks);
  let failed = 0;
  for (const [label, ok] of checks) {
    if (!ok) failed += 1;
    console.log(`${ok ? 'PASS' : 'FAIL'}  ${label}`);
  }
  console.log(`VISUAL_CHECKS total=${checks.length} failed=${failed}`);
  process.exitCode = failed === 0 ? 0 : 1;
};

run().catch((error) => {
  console.error('VISUAL_VERIFY_ERROR', error);
  process.exitCode = 2;
});
