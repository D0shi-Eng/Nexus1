/**
 * NEXUS V1 — SMALL-WINDOW / RTL / THEME LAYOUT MATRIX
 * ---------------------------------------------------
 * يقيس النافذة الأصلية الفعلية (Tauri + WebView2) عند الحد الأدنى المعلن وبمقاسين
 * أكبر، في العربية RTL والإنجليزية LTR، وفي ثيمات فاتح/داكن/تباين عالٍ.
 *
 * لماذا CDP + Browser.setWindowBounds: القياس عبر `page.setViewportSize` يحاكي
 * مقاساً داخل الصفحة فقط ولا يغيّر نافذة Windows الفعلية. هنا نغيّر حدود النافذة
 * الأصلية ثم نعايرها حتى يصبح client area (window.innerWidth/innerHeight) مساوياً
 * تماماً للمقاس المطلوب، وهو ما يراه المستخدم عند الحد الأدنى.
 *
 * ما يُقاس لكل سطح:
 *   - تجاوز المستند أفقياً، وعناصر مرئية تتجاوز عرض النافذة (باستثناء حاويات
 *     التمرير الأفقي المصممة: جداول).
 *   - حصار الحوار داخل النافذة، تمريره الداخلي، ووصول أزرار الحفظ/الإلغاء.
 *   - قص نص عربي غير مبرر (overflow:hidden بلا ellipsis/line-clamp).
 *   - تداخل أزرار التحكم الشقيقة.
 *   - اتجاه RTL/LTR فعلي: موضع الشريط الجانبي وشريط البحث.
 *   - تطبيق الثيم فعلياً (data-theme + لون الخلفية + حدود التباين العالي).
 *   - حصر التركيز داخل الحوار، إغلاق Escape، إعادة التركيز، ومؤشر تركيز مرئي.
 *   - أخطاء console/page.
 *
 * التشغيل:
 *   node tools/e2e/nexus-uat-window.mjs <exePath> --evidence=<dir> [--appdata=<dir>]
 *        [--sizes=980x640,1100x700,1280x820] [--dirs=rtl,ltr]
 *        [--themes=light,dark,contrast] [--skip-approval-setup]
 *
 * ملاحظة بيئة: يجب أن يكون الملف التنفيذي خارج مجلد المشروع (مثل
 * %LOCALAPPDATA%\NEXUS\cargo-target\debug) لأن معزّل ملفات الجلسة يمنع كتابة
 * الثنائيات الموجودة داخل مساحة العمل إلى AppData. هذا يطابق مسار البناء الموثق.
 */
import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import { chromium } from 'playwright-core';

const require = createRequire(import.meta.url);
let DatabaseSync = null;
try {
  ({ DatabaseSync } = require('node:sqlite'));
} catch {
  DatabaseSync = null;
}

const argv = process.argv.slice(2);
const exePath = argv.find((a) => !a.startsWith('--'));
const argOf = (name, fallback) => {
  const found = argv.find((a) => a.startsWith(`--${name}=`));
  return found ? found.slice(name.length + 3) : fallback;
};

const PORT = Number(argOf('port', '9444'));
const EVIDENCE = argOf('evidence', path.join(process.cwd(), '.tmp-uat', 'evidence-window'));
const APPDATA_DIR = argOf('appdata', path.join(process.env.LOCALAPPDATA ?? '', 'com.nexus.obligationos.uatharness'));
const DB_PATH = path.join(APPDATA_DIR, 'nexus-local.db');
const ADMIN_EMAIL = argOf('admin-email', 'uat.admin@example.test');
const ADMIN_PASSWORD = argOf('admin-password', 'Uat!Str0ng#Passw0rd26');
const SKIP_APPROVAL_SETUP = argv.includes('--skip-approval-setup');
const SIZES = argOf('sizes', '980x640,1100x700,1280x820')
  .split(',')
  .map((value) => {
    const [width, height] = value.trim().split('x').map(Number);
    return { label: `${width}x${height}`, width, height };
  });
const DIRS = argOf('dirs', 'rtl,ltr').split(',').map((value) => value.trim()).filter(Boolean);
const THEMES = argOf('themes', 'light,dark,contrast').split(',').map((value) => value.trim()).filter(Boolean);

if (!exePath || !existsSync(exePath)) {
  console.error(`WINDOW_MATRIX_FAILURE: executable not found: ${exePath}`);
  process.exit(2);
}
mkdirSync(EVIDENCE, { recursive: true });

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const report = {
  harness: 'nexus-uat-window',
  startedAt: new Date().toISOString(),
  exe: exePath,
  appdata: APPDATA_DIR,
  sizes: SIZES.map((s) => s.label),
  dirs: DIRS,
  themes: THEMES,
  windowResizeMode: 'unknown',
  devicePixelRatio: null,
  cells: [],
  screens: [],
  consoleErrors: [],
  pageErrors: [],
  skipReasons: [],
  summary: { cells: 0, passed: 0, failed: 0, checks: 0, failedChecks: 0 },
};

const recordCheck = (checks, id, ok, detail) => {
  checks.push({ id, ok: Boolean(ok), detail });
  report.summary.checks += 1;
  if (!ok) report.summary.failedChecks += 1;
  return ok;
};

// ---------------------------------------------------------------------------
// إعدادات اللغة والثيم من الواجهة الحقيقية
// ---------------------------------------------------------------------------
const L = {
  ar: {
    createObligation: 'إنشاء التزام',
    createTask: 'إنشاء مهمة',
    registerEvidence: 'تسجيل دليل مرجعي',
    registerSource: 'تسجيل مصدر تنظيمي',
    sourceSubmit: 'تسجيل المصدر',
    navSettings: 'الإعدادات',
    disable: /^تعطيل$/,
    cancel: /^إلغاء$/,
    decide: /اتخاذ قرار/,
    approvalKind: /موافقة/,
    loginSubmit: 'تسجيل الدخول',
  },
  en: {
    createObligation: 'Create obligation',
    createTask: 'Create task',
    registerEvidence: 'Register evidence reference',
    registerSource: 'Register regulatory source',
    sourceSubmit: 'Register source',
    navSettings: 'Settings',
    disable: /^Disable$/,
    cancel: /^Cancel$/,
    decide: /Make decision|Decide/i,
    approvalKind: /Approval/i,
    loginSubmit: 'Sign in',
  },
};

const activeL = (lang) => L[lang === 'en' ? 'en' : 'ar'];

// ---------------------------------------------------------------------------
// قياس داخل الصفحة (يُنفَّذ في WebView2 الحقيقي)
// ---------------------------------------------------------------------------
const measureInPage = ({ kind, label }) => {
  // هندسة التخطيط الفعلية: clientWidth/clientHeight يستثنيان أشرطة التمرير.
  // المقارنة بـ innerWidth (يشمل الشريط) كانت تُخفي تجاوزاً حقيقياً بمقدار عرض الشريط.
  const vw = document.documentElement.clientWidth;
  const vh = document.documentElement.clientHeight;
  const round = (value) => Math.round(value * 100) / 100;
  const describe = (el) => {
    const tag = el.tagName.toLowerCase();
    const id = el.id ? `#${el.id}` : '';
    const cls = typeof el.className === 'string' && el.className
      ? `.${el.className.trim().split(/\s+/).slice(0, 2).join('.')}`
      : '';
    return `${tag}${id}${cls}`.slice(0, 90);
  };
  const visible = (el) => {
    const style = window.getComputedStyle(el);
    if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') return false;
    const rect = el.getBoundingClientRect();
    return rect.width > 0.5 && rect.height > 0.5;
  };
  const insideScrollableX = (el) => {
    let ancestor = el.parentElement;
    while (ancestor && ancestor !== document.body) {
      const style = window.getComputedStyle(ancestor);
      if ((style.overflowX === 'auto' || style.overflowX === 'scroll')
        && ancestor.scrollWidth > ancestor.clientWidth + 1) {
        const rect = ancestor.getBoundingClientRect();
        if (rect.left >= -1 && rect.right <= vw + 1) return true;
      }
      ancestor = ancestor.parentElement;
    }
    return false;
  };

  // 1) عناصر مرئية تتجاوز عرض النافذة
  const overflowing = [];
  for (const el of document.querySelectorAll('body *')) {
    if (['SCRIPT', 'STYLE', 'LINK', 'META', 'BR', 'TITLE', 'HEAD'].includes(el.tagName)) continue;
    if (el.closest('.dialog-backdrop, .backdrop, .visually-hidden')) continue;
    if (!visible(el)) continue;
    const rect = el.getBoundingClientRect();
    const right = rect.right - vw;
    const left = -rect.left;
    if (right <= 1 && left <= 1) continue;
    if (insideScrollableX(el)) continue;
    if (el.classList.contains('dialog-layer') || el.classList.contains('dialog-backdrop') || el.classList.contains('backdrop')) continue;
    overflowing.push({
      node: describe(el),
      rightOverflowPx: round(Math.max(0, right)),
      leftOverflowPx: round(Math.max(0, left)),
      width: round(rect.width),
      height: round(rect.height),
    });
  }

  // 2) قص نص غير مبرر (overflow:hidden بلا ellipsis/line-clamp)
  const clippedText = [];
  for (const el of document.querySelectorAll('body *')) {
    if (el.closest('.visually-hidden')) continue;
    if (!visible(el)) continue;
    const style = window.getComputedStyle(el);
    if (!(style.overflow === 'hidden' || style.overflowX === 'hidden' || style.overflowY === 'hidden')) continue;
    if (el.scrollWidth <= el.clientWidth + 1) continue;
    if (style.textOverflow === 'ellipsis') continue;
    if (style.webkitLineClamp && style.webkitLineClamp !== 'none') continue;
    const hasDirectText = Array.from(el.childNodes).some(
      (node) => node.nodeType === Node.TEXT_NODE && (node.textContent ?? '').trim().length > 0,
    );
    if (!hasDirectText) continue;
    clippedText.push({
      node: describe(el),
      scrollWidth: el.scrollWidth,
      clientWidth: el.clientWidth,
      text: (el.textContent ?? '').trim().slice(0, 48),
    });
  }

  // 3) تداخل أزرار التحكم الشقيقة (نفس الحاوية)
  const buttonOverlaps = [];
  const containers = new Set();
  for (const button of document.querySelectorAll('button')) {
    if (visible(button) && button.parentElement) containers.add(button.parentElement);
  }
  for (const container of containers) {
    const buttons = Array.from(container.children).filter(
      (child) => child.tagName === 'BUTTON' && visible(child),
    );
    for (let i = 0; i < buttons.length; i += 1) {
      for (let j = i + 1; j < buttons.length; j += 1) {
        const a = buttons[i].getBoundingClientRect();
        const b = buttons[j].getBoundingClientRect();
        const overlapX = Math.min(a.right, b.right) - Math.max(a.left, b.left);
        const overlapY = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top);
        if (overlapX > 2 && overlapY > 2) {
          buttonOverlaps.push({
            a: describe(buttons[i]),
            b: describe(buttons[j]),
            overlapPx: round(overlapX * overlapY),
          });
        }
      }
    }
  }

  // 4) الثيم والاتجاه
  const root = document.documentElement;
  const sidebar = document.querySelector('.sidebar');
  const sidebarRect = sidebar ? sidebar.getBoundingClientRect() : null;
  const search = document.querySelector('.topbar__search');
  const searchRect = search ? search.getBoundingClientRect() : null;

  // 5) عناصر النماذج المرئية (قوائم الاختيار/الحقول)
  const controls = Array.from(document.querySelectorAll('select, input, textarea'))
    .filter((el) => visible(el))
    .map((el) => {
      const rect = el.getBoundingClientRect();
      // عنصر داخل حاوية تمرير أفقي مصمّمة ليس «خارج النافذة»: يمكن الوصول إليه
      // بتمرير الحاوية نفسها (مثل أعمدة جدول أعرض من الشاشة).
      const reachableByScrolling = insideScrollableX(el);
      return {
        tag: el.tagName.toLowerCase(),
        type: el.getAttribute('type') ?? null,
        width: round(rect.width),
        height: round(rect.height),
        outsideHorizontal: !reachableByScrolling && (rect.left < -1 || rect.right > vw + 1),
      };
    });

  // 6) الحوارات
  let dialog = null;
  if (kind === 'dialog') {
    const element = document.querySelector('[role="dialog"]');
    if (element) {
      const rect = element.getBoundingClientRect();
      const style = window.getComputedStyle(element);
      const buttons = Array.from(element.querySelectorAll('button'))
        .filter((button) => visible(button))
        .map((button) => {
          const buttonRect = button.getBoundingClientRect();
          return {
            label: (button.getAttribute('aria-label') ?? button.textContent ?? '').trim().slice(0, 60),
            disabled: button.disabled,
            rect: {
              left: round(buttonRect.left),
              top: round(buttonRect.top),
              right: round(buttonRect.right),
              bottom: round(buttonRect.bottom),
            },
            inViewport: buttonRect.top >= -1 && buttonRect.bottom <= vh + 1
              && buttonRect.left >= -1 && buttonRect.right <= vw + 1,
          };
        });
      const actionElements = Array.from(element.querySelectorAll(
        'footer button, .task-form__actions button, button[type="submit"]',
      )).filter((button) => visible(button));
      const actionButtons = actionElements.map((button) => {
        const buttonRect = button.getBoundingClientRect();
        return {
          label: (button.getAttribute('aria-label') ?? button.textContent ?? '').trim().slice(0, 60),
          disabled: button.disabled,
          rect: {
            left: round(buttonRect.left),
            top: round(buttonRect.top),
            right: round(buttonRect.right),
            bottom: round(buttonRect.bottom),
          },
          inViewport: buttonRect.top >= -1 && buttonRect.bottom <= vh + 1
            && buttonRect.left >= -1 && buttonRect.right <= vw + 1,
        };
      });
      const headerButtons = Array.from(element.querySelectorAll('header button'))
        .filter((button) => visible(button))
        .map((button) => {
          const buttonRect = button.getBoundingClientRect();
          return {
            label: (button.getAttribute('aria-label') ?? button.textContent ?? '').trim().slice(0, 60),
            inViewport: buttonRect.top >= -1 && buttonRect.bottom <= vh + 1
              && buttonRect.left >= -1 && buttonRect.right <= vw + 1,
          };
        });
      dialog = {
        rect: {
          left: round(rect.left),
          top: round(rect.top),
          right: round(rect.right),
          bottom: round(rect.bottom),
        },
        withinViewport: rect.left >= -0.5 && rect.top >= -0.5
          && rect.right <= vw + 0.5 && rect.bottom <= vh + 0.5,
        width: round(rect.width),
        height: round(rect.height),
        overflowX: style.overflowX,
        overflowY: style.overflowY,
        scrollWidth: element.scrollWidth,
        clientWidth: element.clientWidth,
        scrollHeight: element.scrollHeight,
        clientHeight: element.clientHeight,
        horizontalScroll: element.scrollWidth - element.clientWidth,
        verticalScrollable: element.scrollHeight > element.clientHeight + 1,
        buttons,
        actionButtons,
        headerButtons,
        hasBackdrop: Boolean(document.querySelector('.dialog-backdrop')),
      };
    }
  }

  // 7) تنبيهات الأخطاء الظاهرة
  const alerts = Array.from(document.querySelectorAll('.form-error, .error-panel, [role="alert"]'))
    .filter((el) => visible(el))
    .map((el) => {
      const rect = el.getBoundingClientRect();
      return {
        text: (el.textContent ?? '').trim().slice(0, 120),
        withinViewport: rect.left >= -1 && rect.right <= vw + 1 && rect.top >= -1 && rect.bottom <= vh + 1,
        width: round(rect.width),
      };
    });

  return {
    kind,
    label,
    viewport: {
      width: vw,
      height: vh,
      innerWidth: window.innerWidth,
      innerHeight: window.innerHeight,
      dpr: window.devicePixelRatio,
    },
    document: {
      scrollWidth: root.scrollWidth,
      clientWidth: root.clientWidth,
      scrollHeight: root.scrollHeight,
      clientHeight: root.clientHeight,
      horizontalOverflow: root.scrollWidth - root.clientWidth,
      verticalOverflow: root.scrollHeight - root.clientHeight,
    },
    overflowing,
    overflowingCount: overflowing.length,
    clippedText,
    clippedTextCount: clippedText.length,
    buttonOverlaps,
    buttonOverlapCount: buttonOverlaps.length,
    direction: {
      lang: root.getAttribute('lang'),
      dir: root.getAttribute('dir'),
      sidebar: sidebarRect
        ? { left: round(sidebarRect.left), right: round(sidebarRect.right), width: round(sidebarRect.width) }
        : null,
      search: searchRect
        ? { left: round(searchRect.left), right: round(searchRect.right), width: round(searchRect.width) }
        : null,
    },
    theme: {
      attribute: root.getAttribute('data-theme'),
      bodyBackground: window.getComputedStyle(document.body).backgroundColor,
      panelBorderWidth: (() => {
        const panel = document.querySelector('.panel');
        return panel ? window.getComputedStyle(panel).borderTopWidth : null;
      })(),
    },
    controls,
    controlsOutsideHorizontal: controls.filter((control) => control.outsideHorizontal).length,
    dialog,
    alerts,
    hasShell: Boolean(document.querySelector('.app-shell')),
    activeNavCount: document.querySelectorAll('.navigation__item--active').length,
    heading: document.querySelector('main h1')?.textContent?.trim() ?? null,
    bodyTextLength: (document.querySelector('main')?.innerText ?? '').trim().length,
  };
};

// ---------------------------------------------------------------------------
// التشغيل
// ---------------------------------------------------------------------------
const waitForCdp = async (port, timeoutMs = 90000) => {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/version`);
      if (response.ok) return await response.json();
    } catch {
      /* لم يُفتح بعد */
    }
    await sleep(400);
  }
  throw new Error(`CDP on port ${port} did not open within ${timeoutMs}ms`);
};

const child = spawn(exePath, [], {
  env: {
    ...process.env,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT} --remote-allow-origins=http://127.0.0.1:${PORT}`,
  },
  stdio: 'ignore',
  detached: false,
});

let browser = null;
let exitCode = 1;
try {
  const version = await waitForCdp(PORT);
  report.browser = { Browser: version.Browser };
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${PORT}`);
  const context = browser.contexts()[0] ?? (await browser.newContext());
  let page = context.pages().find((candidate) => !candidate.url().startsWith('devtools://'));
  if (!page) {
    const deadline = Date.now() + 30000;
    while (!page && Date.now() < deadline) {
      await sleep(300);
      page = context.pages().find((candidate) => !candidate.url().startsWith('devtools://'));
    }
  }
  if (!page) throw new Error('no application page found over CDP');
  page.on('console', (message) => {
    if (message.type() === 'error') report.consoleErrors.push(message.text().slice(0, 400));
  });
  page.on('pageerror', (error) => report.pageErrors.push(String(error.message ?? error).slice(0, 400)));
  await page.waitForLoadState('domcontentloaded');
  await page.waitForSelector('#root', { timeout: 45000 });

  const pageInfo = await fetch(`http://127.0.0.1:${PORT}/json/list`).then((response) => response.json());
  const pageTarget = pageInfo.find((target) => target.type === 'page' && !target.url.startsWith('devtools://'));

  // إعداد جلسة CDP على مستوى المتصفح لتغيير حدود النافذة الأصلية.
  let browserSession = null;
  let windowId = null;
  try {
    browserSession = await browser.newBrowserCDPSession();
    const info = await browserSession.send('Browser.getWindowForTarget', { targetId: pageTarget.id });
    windowId = info.windowId;
    report.windowResizeMode = 'native-cdp';
  } catch (error) {
    report.windowResizeMode = 'unavailable';
    report.skipReasons.push(`native window resize unavailable: ${String(error.message ?? error).slice(0, 200)}`);
  }

  const readViewport = () => page.evaluate(() => ({ width: window.innerWidth, height: window.innerHeight, dpr: window.devicePixelRatio }));
  const setClientSize = async (width, height) => {
    if (!browserSession || !windowId) throw new Error('native window resize unavailable');
    let bounds = (await browserSession.send('Browser.getWindowBounds', { windowId })).bounds;
    if (bounds.windowState !== 'normal') {
      await browserSession.send('Browser.setWindowBounds', { windowId, bounds: { windowState: 'normal' } });
      await sleep(300);
      bounds = (await browserSession.send('Browser.getWindowBounds', { windowId })).bounds;
    }
    let outerWidth = bounds.width;
    let outerHeight = bounds.height;
    let viewport = await readViewport();
    for (let attempt = 0; attempt < 10; attempt += 1) {
      const deltaWidth = width - viewport.width;
      const deltaHeight = height - viewport.height;
      if (deltaWidth === 0 && deltaHeight === 0) break;
      outerWidth = Math.max(400, outerWidth + deltaWidth);
      outerHeight = Math.max(300, outerHeight + deltaHeight);
      await browserSession.send('Browser.setWindowBounds', {
        windowId,
        bounds: { width: Math.round(outerWidth), height: Math.round(outerHeight) },
      });
      await sleep(350);
      viewport = await readViewport();
    }
    return viewport;
  };

  // ---------- الدخول إلى مساحة الإنتاج المعزولة ----------
  const ensureAuthenticated = async () => {
    if (await page.locator('#login-email').count() > 0) {
      const lang = await page.evaluate(() => document.documentElement.getAttribute('lang') ?? 'ar');
      await page.fill('#login-email', ADMIN_EMAIL);
      await page.fill('#login-password', ADMIN_PASSWORD);
      await page.getByRole('button', { name: activeL(lang).loginSubmit, exact: true }).first().click();
    }
    await page.waitForSelector('nav.navigation', { timeout: 60000 });
    await page.waitForSelector('.app-shell', { timeout: 60000 });
    await sleep(700);
  };

  if (await page.locator('#setup-title').count() > 0) {
    throw new Error('container is UNPROVISIONED — run the production UAT against this isolated container first');
  }
  await ensureAuthenticated();
  report.devicePixelRatio = (await readViewport()).dpr;

  // ---------- بيانات الإعداد: اعتماد معلّق حقيقي عبر أوامر التطبيق ----------
  const ipc = (command, args) => page.evaluate(
    async ([cmd, payload]) => {
      try {
        return { ok: true, result: await window.__TAURI_INTERNALS__.invoke(cmd, payload) };
      } catch (error) {
        return { ok: false, code: error?.code ?? 'UNKNOWN', message: String(error?.message ?? error).slice(0, 160) };
      }
    },
    [command, args ?? {}],
  );

  let pendingApprovalReady = false;
  if (!SKIP_APPROVAL_SETUP) {
    try {
      const dashboard = await ipc('get_dashboard');
      const obligations = dashboard.result?.obligations ?? [];
      const obligation = obligations.find((item) => item.status !== 'ARCHIVED') ?? obligations[0];
      const users = await ipc('list_workspace_users');
      const employee = (users.result?.users ?? []).find((user) => user.email === 'uat.operator@example.test');
      if (obligation && employee) {
        const created = await ipc('create_remediation_task', {
          input: {
            obligationId: obligation.id,
            title: 'UAT layout matrix approval task',
            description: 'Isolated setup record for the small-window matrix. No product data.',
            dueAt: null,
            priority: 'LOW',
            assigneeUserId: employee.userId,
          },
        });
        if (created.ok) {
          const first = await ipc('transition_task', {
            input: {
              taskId: created.result.taskId,
              targetStatus: 'IN_PROGRESS',
              reason: 'layout matrix setup',
              expectedVersion: created.result.status ? 1 : 1,
            },
          });
          const second = await ipc('transition_task', {
            input: {
              taskId: created.result.taskId,
              targetStatus: 'READY_FOR_REVIEW',
              reason: 'layout matrix setup',
              expectedVersion: 2,
            },
          });
          pendingApprovalReady = Boolean(first.ok && second.ok && second.result?.pendingApprovalId);
          if (!pendingApprovalReady) {
            report.skipReasons.push(`approval setup transitions failed: ${JSON.stringify({ first, second }).slice(0, 300)}`);
          }
        } else {
          report.skipReasons.push(`approval setup task creation failed: ${created.code ?? 'UNKNOWN'}`);
        }
      } else {
        report.skipReasons.push('approval setup skipped: no obligation/employee found in the isolated container');
      }
    } catch (error) {
      report.skipReasons.push(`approval setup error: ${String(error.message ?? error).slice(0, 200)}`);
    }
    if (pendingApprovalReady) {
      await page.reload({ waitUntil: 'domcontentloaded' });
      await ensureAuthenticated();
    }
  }
  report.pendingApprovalReady = pendingApprovalReady;

  // ---------- مصادر بيانات اختبار الخطأ (قراءة فقط من قاعدة الاختبار) ----------
  let sourceFixture = null;
  if (DatabaseSync && existsSync(DB_PATH)) {
    try {
      const database = new DatabaseSync(DB_PATH, { readOnly: true });
      sourceFixture = database
        .prepare('SELECT authority, document_title, document_version FROM regulatory_sources ORDER BY rowid ASC LIMIT 1')
        .get();
      database.close();
    } catch (error) {
      report.skipReasons.push(`source fixture unavailable: ${String(error.message ?? error).slice(0, 160)}`);
      sourceFixture = null;
    }
  }

  // ---------- أدوات الواجهة ----------
  const currentLang = () => page.evaluate(() => document.documentElement.getAttribute('lang') ?? 'ar');
  const setLanguage = async (target) => {
    for (let attempt = 0; attempt < 4; attempt += 1) {
      const lang = await currentLang();
      if (lang === target) break;
      await page.locator('.language-button').first().click();
      await sleep(700);
    }
    const lang = await currentLang();
    if (lang !== target) throw new Error(`language did not switch to ${target} (got ${lang})`);
  };

  const closeAnyDialog = async () => {
    for (let attempt = 0; attempt < 3; attempt += 1) {
      const count = await page.locator('[role="dialog"]').count();
      if (count === 0) return;
      await page.keyboard.press('Escape');
      await sleep(350);
    }
    if (await page.locator('[role="dialog"]').count() > 0) {
      await page.locator('.dialog-backdrop').first().click({ position: { x: 5, y: 5 } }).catch(() => {});
      await sleep(300);
    }
  };

  const openSettingsSection = async (index) => {
    await closeAnyDialog();
    await page.locator('.navigation__item').nth(14).click();
    await page.waitForSelector('.settings-layout', { timeout: 30000 });
    await sleep(300);
    await page.locator('.settings-nav__item').nth(index).click();
    await sleep(450);
  };

  const setTheme = async (theme) => {
    await openSettingsSection(1);
    const radio = page.locator(`input[name="nexus-theme"][value="${theme}"]`).first();
    await radio.waitFor({ state: 'attached', timeout: 15000 });
    await radio.check({ force: true });
    await sleep(450);
    const applied = await page.evaluate(() => document.documentElement.getAttribute('data-theme'));
    return applied;
  };

  const collect = async (kind, label, screenshotName) => {
    await page.evaluate(() => document.fonts?.ready ?? null).catch(() => {});
    await sleep(kind === 'dialog' ? 350 : 250);
    const measurement = await page.evaluate(measureInPage, { kind, label });
    if (screenshotName) {
      await page.screenshot({ path: path.join(EVIDENCE, `${screenshotName}.png`) }).catch(() => {});
    }
    return measurement;
  };

  const navTo = async (index, expectedLang) => {
    await closeAnyDialog();
    await page.locator('.navigation__item').nth(index).click();
    await sleep(500);
    return page.evaluate(() => ({
      heading: document.querySelector('main h1')?.textContent?.trim() ?? null,
      activeNavCount: document.querySelectorAll('.navigation__item--active').length,
      lang: document.documentElement.getAttribute('lang'),
    }));
  };

  const openDialogByText = async (textPattern, options = {}) => {
    const scope = options.scope ? page.locator(options.scope) : page.locator('main');
    const button = scope.getByRole('button', { name: textPattern }).first();
    await button.waitFor({ state: 'visible', timeout: 20000 });
    await button.click();
    await page.locator('[role="dialog"]').first().waitFor({ state: 'visible', timeout: 20000 });
    await sleep(500);
    return page.locator('[role="dialog"]').first();
  };

  const focusTrapTest = async () => {
    const inside = () => page.evaluate(() => {
      const dialog = document.querySelector('[role="dialog"]');
      const active = document.activeElement;
      return Boolean(dialog && active && dialog.contains(active));
    });
    for (let index = 0; index < 12; index += 1) {
      await page.keyboard.press('Tab');
      await sleep(60);
      if (!(await inside())) return { ok: false, step: index, escaped: true };
    }
    return { ok: true };
  };

  const escapeClosesTest = async () => {
    await page.keyboard.press('Escape');
    await sleep(400);
    const count = await page.locator('[role="dialog"]').count();
    const focus = await page.evaluate(() => {
      const active = document.activeElement;
      return active ? { tag: active.tagName, inBody: document.body.contains(active) } : null;
    });
    return { closed: count === 0, focus };
  };

  const evaluateScreenChecks = (checks, measurement, expectedDir) => {
    const layoutWidth = measurement.document.clientWidth;
    recordCheck(checks, `${measurement.label}:no-document-horizontal-overflow`, measurement.document.horizontalOverflow <= 1,
      `scrollWidth=${measurement.document.scrollWidth} clientWidth=${measurement.document.clientWidth}`);
    recordCheck(checks, `${measurement.label}:no-element-overflows-viewport`, measurement.overflowingCount === 0,
      `${measurement.overflowingCount} overflowing: ${JSON.stringify(measurement.overflowing.slice(0, 4))}`);
    recordCheck(checks, `${measurement.label}:no-unjustified-text-clipping`, measurement.clippedTextCount === 0,
      `${measurement.clippedTextCount} clipped: ${JSON.stringify(measurement.clippedText.slice(0, 4))}`);
    recordCheck(checks, `${measurement.label}:no-control-button-overlap`, measurement.buttonOverlapCount === 0,
      `${measurement.buttonOverlapCount} overlaps: ${JSON.stringify(measurement.buttonOverlaps.slice(0, 4))}`);
    recordCheck(checks, `${measurement.label}:shell-rendered`, measurement.hasShell && measurement.bodyTextLength > 0,
      `shell=${measurement.hasShell} chars=${measurement.bodyTextLength}`);
    recordCheck(checks, `${measurement.label}:single-active-nav`, measurement.activeNavCount === 1,
      `activeNavCount=${measurement.activeNavCount}`);
    recordCheck(checks, `${measurement.label}:no-controls-outside-horizontal`, measurement.controlsOutsideHorizontal === 0,
      `outside=${measurement.controlsOutsideHorizontal} of ${measurement.controls.length}`);
    const dir = measurement.direction;
    recordCheck(checks, `${measurement.label}:direction-attribute`, dir.dir === (expectedDir === 'rtl' ? 'rtl' : 'ltr'),
      `dir=${dir.dir} lang=${dir.lang}`);
    if (dir.sidebar && layoutWidth > 800) {
      if (expectedDir === 'rtl') {
        recordCheck(checks, `${measurement.label}:rtl-sidebar-on-right`,
          dir.sidebar.left > layoutWidth / 2 && dir.sidebar.right >= layoutWidth - 2,
          JSON.stringify(dir.sidebar));
      } else {
        recordCheck(checks, `${measurement.label}:ltr-sidebar-on-left`,
          dir.sidebar.left <= 2 && dir.sidebar.right < layoutWidth / 2,
          JSON.stringify(dir.sidebar));
      }
    }
    if (dir.search) {
      if (expectedDir === 'rtl') {
        recordCheck(checks, `${measurement.label}:rtl-search-start-side`,
          dir.search.right > layoutWidth / 2, JSON.stringify(dir.search));
      } else {
        recordCheck(checks, `${measurement.label}:ltr-search-start-side`,
          dir.search.left < layoutWidth / 2, JSON.stringify(dir.search));
      }
    }
  };

  const checkActionReachability = async (label) => page.evaluate(() => {
    const visible = (el) => {
      const style = window.getComputedStyle(el);
      if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') return false;
      const rect = el.getBoundingClientRect();
      return rect.width > 0.5 && rect.height > 0.5;
    };
    const collectButtons = (dialog) => Array.from(dialog.querySelectorAll(
      'footer button, .task-form__actions button, button[type="submit"]',
    )).filter(visible).map((button) => {
      const rect = button.getBoundingClientRect();
      return {
        label: (button.getAttribute('aria-label') ?? button.textContent ?? '').trim().slice(0, 60),
        inViewport: rect.top >= -1 && rect.bottom <= document.documentElement.clientHeight + 1
          && rect.left >= -1 && rect.right <= document.documentElement.clientWidth + 1,
      };
    });
    const dialog = document.querySelector('[role="dialog"]');
    if (!dialog) return null;
    const initial = collectButtons(dialog);
    dialog.scrollTop = dialog.scrollHeight;
    return new Promise((resolve) => {
      window.requestAnimationFrame(() => {
        resolve({ initial, afterScroll: collectButtons(dialog), scrollTop: dialog.scrollTop });
      });
    });
  });

  const evaluateDialogChecks = async (checks, measurement, expectedDir, options = {}) => {
    const dialog = measurement.dialog;
    recordCheck(checks, `${measurement.label}:dialog-present`, Boolean(dialog), 'no [role=dialog] found');
    if (!dialog) return;
    evaluateScreenChecks(checks, measurement, expectedDir);
    recordCheck(checks, `${measurement.label}:dialog-within-viewport`, dialog.withinViewport,
      `rect=${JSON.stringify(dialog.rect)} viewport=${JSON.stringify(measurement.viewport)}`);
    recordCheck(checks, `${measurement.label}:dialog-no-horizontal-scroll`, dialog.horizontalScroll <= 1,
      `scrollWidth=${dialog.scrollWidth} clientWidth=${dialog.clientWidth}`);
    const minimumActions = options.minimumActions ?? 2;
    recordCheck(checks, `${measurement.label}:dialog-has-actions`,
      dialog.actionButtons.length >= minimumActions,
      `${dialog.actionButtons.length} action buttons: ${JSON.stringify(dialog.actionButtons.map((b) => b.label))}`);
    recordCheck(checks, `${measurement.label}:dialog-backdrop`, dialog.hasBackdrop, 'no .dialog-backdrop');
    recordCheck(checks, `${measurement.label}:dialog-close-visible`,
      Array.isArray(dialog.headerButtons) && dialog.headerButtons.length > 0 && dialog.headerButtons.some((button) => button.inViewport),
      JSON.stringify(dialog.headerButtons));
    recordCheck(checks, `${measurement.label}:alerts-within-viewport`,
      measurement.alerts.every((alert) => alert.withinViewport),
      JSON.stringify(measurement.alerts));
    // أزرار الحفظ/الإلغاء: يجب أن تكون مرئية مباشرة أو قابلة للوصول بتمرير الحوار نفسه.
    const reach = await checkActionReachability(measurement.label);
    recordCheck(checks, `${measurement.label}:dialog-actions-reachable`,
      Boolean(reach) && reach.afterScroll.every((button) => button.inViewport),
      reach ? `initialHidden=${JSON.stringify(reach.initial.filter((b) => !b.inViewport).map((b) => b.label))} scrollTop=${reach.scrollTop}` : 'no dialog');
  };

  // ---------- المصفوفة ----------
  for (const size of SIZES) {
    let actualViewport = null;
    try {
      actualViewport = await setClientSize(size.width, size.height);
    } catch (error) {
      report.skipReasons.push(`resize ${size.label} failed: ${String(error.message ?? error).slice(0, 200)}`);
      continue;
    }
    for (const dir of DIRS) {
      const targetLang = dir === 'rtl' ? 'ar' : 'en';
      const labels = activeL(targetLang);
      try {
        await setLanguage(targetLang);
      } catch (error) {
        report.skipReasons.push(`language ${targetLang} failed: ${String(error.message ?? error).slice(0, 160)}`);
        continue;
      }
      for (const theme of THEMES) {
        const cell = {
          size: size.label,
          requested: { width: size.width, height: size.height },
          actualViewport,
          dir,
          lang: targetLang,
          theme,
          checks: [],
          measurements: [],
          status: 'FAIL',
          error: null,
        };
        const checks = cell.checks;
        const started = Date.now();
        try {
          const appliedTheme = await setTheme(theme);
          recordCheck(checks, `cell:theme-applied`, appliedTheme === theme, `data-theme=${appliedTheme}`);
          recordCheck(checks, `cell:viewport-exact`, actualViewport.width === size.width && actualViewport.height === size.height,
            `inner=${actualViewport.width}x${actualViewport.height} requested=${size.width}x${size.height}`);
          const firstThemeMeasurement = await collect('screen', 'settings-appearance', `m-${size.label}-${dir}-${theme}-settings-appearance`);
          cell.measurements.push(firstThemeMeasurement);
          evaluateScreenChecks(checks, firstThemeMeasurement, dir);
          recordCheck(checks, `settings-appearance:theme-colors`, Boolean(firstThemeMeasurement.theme.bodyBackground),
            `data-theme=${firstThemeMeasurement.theme.attribute} body=${firstThemeMeasurement.theme.bodyBackground}`);
          if (theme === 'contrast' && firstThemeMeasurement.theme.panelBorderWidth) {
            recordCheck(checks, `settings-appearance:contrast-panel-border`,
              parseFloat(firstThemeMeasurement.theme.panelBorderWidth) >= 2,
              `borderTopWidth=${firstThemeMeasurement.theme.panelBorderWidth}`);
          }

          // شاشات الوحدات المطلوبة
          const screenPlan = [
            { key: 'settings-users', nav: 14, settingsIndex: 5 },
            { key: 'overview', nav: 0 },
            { key: 'attention', nav: 1 },
            { key: 'obligations', nav: 2 },
            { key: 'regulatory', nav: 3 },
            { key: 'evidence', nav: 5 },
            { key: 'tasks', nav: 6 },
            { key: 'reports', nav: 10 },
            { key: 'audit', nav: 11 },
          ];
          for (const screen of screenPlan) {
            if (screen.settingsIndex !== undefined) {
              await openSettingsSection(screen.settingsIndex);
            } else {
              await navTo(screen.nav, targetLang);
            }
            const measurement = await collect('screen', screen.key, `m-${size.label}-${dir}-${theme}-${screen.key}`);
            cell.measurements.push(measurement);
            evaluateScreenChecks(checks, measurement, dir);
          }

          // حوار إنشاء الالتزام + حصر التركيز + Escape
          await navTo(2, targetLang);
          await openDialogByText(new RegExp(`^${labels.createObligation}$`));
          let measurement = await collect('dialog', 'dialog-create-obligation', `m-${size.label}-${dir}-${theme}-dialog-create-obligation`);
          cell.measurements.push(measurement);
          await evaluateDialogChecks(checks, measurement, dir);
          const trap = await focusTrapTest();
          recordCheck(checks, 'dialog-create-obligation:focus-trap', trap.ok, JSON.stringify(trap));
          const escape = await escapeClosesTest();
          recordCheck(checks, 'dialog-create-obligation:escape-closes', escape.closed, JSON.stringify(escape));
          recordCheck(checks, 'dialog-create-obligation:focus-restored', Boolean(escape.focus?.inBody), JSON.stringify(escape.focus));

          // حوار إنشاء مصدر تنظيمي + تنبيه خطأ حقيقي (تكرار مصدر قائم)
          await navTo(3, targetLang);
          await openDialogByText(new RegExp(`^${labels.registerSource}$`));
          measurement = await collect('dialog', 'dialog-create-source', `m-${size.label}-${dir}-${theme}-dialog-create-source`);
          cell.measurements.push(measurement);
          await evaluateDialogChecks(checks, measurement, dir);
          if (sourceFixture) {
            const inputs = page.locator('[role="dialog"] input:not([type="hidden"])');
            await inputs.nth(0).fill(String(sourceFixture.authority ?? ''));
            await inputs.nth(1).fill(String(sourceFixture.document_title ?? ''));
            if (await inputs.count() > 2 && sourceFixture.document_version) {
              await inputs.nth(2).fill(String(sourceFixture.document_version));
            }
            await page.locator('[role="dialog"]').getByRole('button', { name: new RegExp(`^${labels.sourceSubmit}$`) }).first().click();
            await page.locator('[role="dialog"] .form-error').first().waitFor({ state: 'visible', timeout: 15000 }).catch(() => {});
            await sleep(400);
            measurement = await collect('dialog', 'dialog-error-alert', `m-${size.label}-${dir}-${theme}-dialog-error-alert`);
            cell.measurements.push(measurement);
            await evaluateDialogChecks(checks, measurement, dir);
            recordCheck(checks, 'dialog-error-alert:visible', measurement.alerts.length > 0,
              `${measurement.alerts.length} alerts`);
          } else {
            recordCheck(checks, 'dialog-error-alert:visible', false, 'source fixture unavailable in isolated DB');
          }
          await closeAnyDialog();

          // حوار المهمة وحوار الدليل
          await navTo(0, targetLang);
          await openDialogByText(new RegExp(`^${labels.createTask}$`));
          measurement = await collect('dialog', 'dialog-create-task', `m-${size.label}-${dir}-${theme}-dialog-create-task`);
          cell.measurements.push(measurement);
          await evaluateDialogChecks(checks, measurement, dir);
          await closeAnyDialog();
          await openDialogByText(new RegExp(`^${labels.registerEvidence}$`));
          measurement = await collect('dialog', 'dialog-register-evidence', `m-${size.label}-${dir}-${theme}-dialog-register-evidence`);
          cell.measurements.push(measurement);
          await evaluateDialogChecks(checks, measurement, dir);
          await closeAnyDialog();

          // لوحة الأوامر: حصر التركيز + Escape
          await page.keyboard.press('Control+k');
          await page.locator('.search-dialog').first().waitFor({ state: 'visible', timeout: 15000 });
          measurement = await collect('dialog', 'dialog-command-palette', `m-${size.label}-${dir}-${theme}-dialog-command-palette`);
          cell.measurements.push(measurement);
          await evaluateDialogChecks(checks, measurement, dir, { minimumActions: 0 });
          const searchTrap = await focusTrapTest();
          recordCheck(checks, 'dialog-command-palette:focus-trap', searchTrap.ok, JSON.stringify(searchTrap));
          const searchEscape = await escapeClosesTest();
          recordCheck(checks, 'dialog-command-palette:escape-closes', searchEscape.closed, JSON.stringify(searchEscape));

          // حوار تأكيد الدوم (تعطيل عضو) ثم إلغاء
          await openSettingsSection(5);
          const disableButtons = page.locator('#settings table tbody tr button', { hasText: labels.disable });
          let confirmOpened = false;
          const disableCount = await disableButtons.count();
          for (let index = 0; index < disableCount; index += 1) {
            const candidate = disableButtons.nth(index);
            if (!(await candidate.isEnabled().catch(() => false))) continue;
            await candidate.click();
            await page.locator('[role="dialog"]').first().waitFor({ state: 'visible', timeout: 10000 }).catch(() => {});
            confirmOpened = (await page.locator('[role="dialog"]').count()) > 0;
            break;
          }
          recordCheck(checks, 'dialog-confirm:opened', confirmOpened, `${disableCount} disable controls`);
          if (confirmOpened) {
            measurement = await collect('dialog', 'dialog-confirm', `m-${size.label}-${dir}-${theme}-dialog-confirm`);
            cell.measurements.push(measurement);
            await evaluateDialogChecks(checks, measurement, dir);
            const cancelButton = page.locator('[role="dialog"]').getByRole('button', { name: labels.cancel }).first();
            await cancelButton.click();
            await sleep(400);
            recordCheck(checks, 'dialog-confirm:cancel-closes', (await page.locator('[role="dialog"]').count()) === 0, 'cancel did not close');
          }

          // حوار قرار الاعتماد (إن وُجد اعتماد معلّق في حاوية الاختبار)
          if (pendingApprovalReady) {
            await navTo(1, targetLang);
            const approvalCard = page.locator('.attention-card').filter({ hasText: labels.approvalKind }).first();
            const cardCount = await approvalCard.count();
            let opened = false;
            if (cardCount > 0) {
              const decide = approvalCard.getByRole('button', { name: labels.decide }).first();
              if (await decide.count() > 0) {
                await decide.click();
                await page.locator('[role="dialog"]').first().waitFor({ state: 'visible', timeout: 10000 }).catch(() => {});
                opened = (await page.locator('[role="dialog"]').count()) > 0;
              }
            }
            recordCheck(checks, 'dialog-approval-decision:opened', opened, `approval cards=${cardCount}`);
            if (opened) {
              measurement = await collect('dialog', 'dialog-approval-decision', `m-${size.label}-${dir}-${theme}-dialog-approval-decision`);
              cell.measurements.push(measurement);
              await evaluateDialogChecks(checks, measurement, dir);
              await closeAnyDialog();
            }
          }

          // مؤشر تركيز مرئي عبر Tab الحقيقي
          await navTo(0, targetLang);
          await page.keyboard.press('Tab');
          await sleep(120);
          const focusRing = await page.evaluate(() => {
            const style = window.getComputedStyle(document.activeElement);
            return {
              tag: document.activeElement?.tagName ?? null,
              outlineStyle: style.outlineStyle,
              outlineWidth: style.outlineWidth,
              boxShadow: style.boxShadow,
            };
          });
          recordCheck(checks, 'keyboard:focus-visible-ring',
            focusRing.outlineStyle !== 'none' && (parseFloat(focusRing.outlineWidth) > 0 || focusRing.boxShadow !== 'none'),
            JSON.stringify(focusRing));

          cell.status = checks.every((check) => check.ok) ? 'PASS' : 'FAIL';
        } catch (error) {
          cell.error = String(error.message ?? error).slice(0, 500);
        } finally {
          cell.durationMs = Date.now() - started;
          if (cell.status !== 'PASS' && !cell.error) cell.status = 'FAIL';
          report.cells.push(cell);
          report.summary.cells += 1;
          if (cell.status === 'PASS') report.summary.passed += 1;
          else report.summary.failed += 1;
          console.log(`[${cell.status}] ${size.label} ${dir} ${theme} — checks=${checks.filter((c) => c.ok).length}/${checks.length}${cell.error ? ` ERROR=${cell.error}` : ''}`);
          await closeAnyDialog().catch(() => {});
        }
      }
    }
  }

  report.consoleErrors = [...new Set(report.consoleErrors)];
  report.pageErrors = [...new Set(report.pageErrors)];
  report.finishedAt = new Date().toISOString();
  report.verdict = report.summary.failed === 0
    && report.summary.failedChecks === 0
    && report.pageErrors.length === 0
    && report.consoleErrors.length === 0
    && report.windowResizeMode === 'native-cdp'
    ? 'PASS'
    : 'FAIL';
  writeFileSync(path.join(EVIDENCE, 'window-matrix-report.json'), JSON.stringify(report, null, 2), 'utf8');
  console.log(`WINDOW_MATRIX_SUMMARY cells=${report.summary.cells} passed=${report.summary.passed} failed=${report.summary.failed} checks=${report.summary.checks} failed_checks=${report.summary.failedChecks} resize_mode=${report.windowResizeMode} verdict=${report.verdict}`);
  if (report.consoleErrors.length > 0) console.log(`CONSOLE_ERRORS=${JSON.stringify(report.consoleErrors.slice(0, 5))}`);
  if (report.pageErrors.length > 0) console.log(`PAGE_ERRORS=${JSON.stringify(report.pageErrors.slice(0, 5))}`);
  if (report.skipReasons.length > 0) console.log(`SKIP_REASONS=${JSON.stringify(report.skipReasons.slice(0, 8))}`);
  exitCode = report.verdict === 'PASS' ? 0 : 1;
} catch (error) {
  report.fatal = String(error.message ?? error).slice(0, 900);
  report.finishedAt = new Date().toISOString();
  report.verdict = 'ERROR';
  writeFileSync(path.join(EVIDENCE, 'window-matrix-report.json'), JSON.stringify(report, null, 2), 'utf8');
  console.error(`WINDOW_MATRIX_FATAL: ${report.fatal}`);
  exitCode = 1;
} finally {
  try {
    if (browser) await browser.close();
  } catch {
    /* إغلاق أفضل جهد */
  }
  try {
    child.kill();
  } catch {
    /* العملية قد انتهت */
  }
  process.exit(exitCode);
}
