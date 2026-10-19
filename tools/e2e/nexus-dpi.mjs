/**
 * NEXUS DPI verification — measures the REAL Tauri WebView2 window at several
 * device scale factors (100% / 125% / 150%) over CDP.
 *
 * لماذا `--force-device-scale-factor`: تغيير DPI العام لجهاز المالك ممنوع في هذه
 * الجولة، وهذا المفتاح يجعل محرك Chromium داخل WebView2 يعمل بنسبة تكبير الجهاز
 * نفسها التي يفرضها Windows عند 125%/150% (نفس حساب CSS px والخطوط والتخطيط)،
 * بلا مساس بإعدادات النظام. القياس هنا امتداد للـWebView وللنافذة، وليس تغييراً
 * لـDPI النظام — وهذا يعلن صراحةً ولا يُخفى.
 *
 * القياسات: devicePixelRatio الفعلي، تجاوز أفقي للمستند، عناصر تتجاوز عرض النافذة،
 * أحجام خطوط ونقاط النقر الأساسية، وظهور قشرة التطبيق/شاشة الدخول/التأسيس.
 *
 * Usage:
 *   node tools/e2e/nexus-dpi.mjs --port=9460 --scale=1.25 --out=<REPO_ROOT>/.tmp-uat/evidence-dpi
 */
import { chromium } from 'playwright-core';
import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';

const argv = process.argv.slice(2);
const argOf = (name, fallback) => {
  const found = argv.find((a) => a.startsWith(`--${name}=`));
  return found ? found.slice(name.length + 3) : fallback;
};

const PORT = Number(argOf('port', '9460'));
const SCALE = Number(argOf('scale', '1'));
const OUT = argOf('out', path.join(process.cwd(), '.tmp-uat', 'evidence-dpi'));
const TIMEOUT = Number(argOf('timeout', '90000'));
mkdirSync(OUT, { recursive: true });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const waitForCdp = async () => {
  const deadline = Date.now() + TIMEOUT;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`http://127.0.0.1:${PORT}/json/version`);
      if (res.ok) return await res.json();
    } catch {
      /* لم يُفتح بعد */
    }
    await sleep(400);
  }
  throw new Error(`CDP did not open on ${PORT} within ${TIMEOUT}ms`);
};

const main = async () => {
  const version = await waitForCdp();
  const browser = await chromium.connectOverCDP(`http://127.0.0.1:${PORT}`);
  const contexts = browser.contexts();
  if (contexts.length === 0) throw new Error('NO_BROWSER_CONTEXT');
  const page = contexts[0].pages()[0];
  if (!page) throw new Error('NO_PAGE');
  await page.waitForSelector('body', { timeout: 30000 });
  // استقرار التخطيط والخطوط قبل القياس
  await page.waitForTimeout(1500);

  const measurements = await page.evaluate(() => {
    const de = document.documentElement;
    const viewportWidth = window.innerWidth;
    const viewportHeight = window.innerHeight;

    const overflowing = [];
    const all = Array.from(document.querySelectorAll('body *'));
    for (const el of all) {
      const style = window.getComputedStyle(el);
      if (style.display === 'none' || style.visibility === 'hidden') continue;
      const rect = el.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) continue;
      const rightOverflow = rect.right - viewportWidth;
      const leftOverflow = -rect.left;
      if (rightOverflow > 1 || leftOverflow > 1) {
        const tag = el.tagName.toLowerCase();
        const id = el.id ? `#${el.id}` : '';
        const cls = typeof el.className === 'string' && el.className ? `.${el.className.trim().split(/\s+/).slice(0, 2).join('.')}` : '';
        overflowing.push({
          node: `${tag}${id}${cls}`.slice(0, 90),
          rightOverflowPx: Math.round(Math.max(0, rightOverflow) * 100) / 100,
          leftOverflowPx: Math.round(Math.max(0, leftOverflow) * 100) / 100,
          width: Math.round(rect.width),
          height: Math.round(rect.height),
        });
      }
    }

    const readFont = (selector) => {
      const el = document.querySelector(selector);
      if (!el) return null;
      const style = window.getComputedStyle(el);
      const rect = el.getBoundingClientRect();
      return {
        selector,
        fontSizePx: parseFloat(style.fontSize),
        lineHeight: style.lineHeight,
        height: Math.round(rect.height * 100) / 100,
        visible: rect.width > 0 && rect.height > 0,
      };
    };

    const visible = (selector) => {
      const el = document.querySelector(selector);
      if (!el) return false;
      const rect = el.getBoundingClientRect();
      const style = window.getComputedStyle(el);
      return rect.width > 0 && rect.height > 0 && style.display !== 'none' && style.visibility !== 'hidden';
    };

    const interactive = Array.from(document.querySelectorAll('button, a, input, select, textarea'))
      .filter((el) => {
        const rect = el.getBoundingClientRect();
        return rect.width > 0 && rect.height > 0;
      })
      .map((el) => {
        const rect = el.getBoundingClientRect();
        return {
          tag: el.tagName.toLowerCase(),
          label: (el.getAttribute('aria-label') || el.textContent || '').trim().slice(0, 60),
          width: Math.round(rect.width * 100) / 100,
          height: Math.round(rect.height * 100) / 100,
        };
      });

    const smallTargets = interactive.filter((t) => t.height < 24 || t.width < 24);

    return {
      devicePixelRatio: window.devicePixelRatio,
      viewport: { width: viewportWidth, height: viewportHeight },
      document: {
        scrollWidth: de.scrollWidth,
        clientWidth: de.clientWidth,
        scrollHeight: de.scrollHeight,
        clientHeight: de.clientHeight,
        horizontalOverflow: de.scrollWidth - de.clientWidth,
      },
      overflowingElements: overflowing.slice(0, 25),
      overflowingCount: overflowing.length,
      fonts: [readFont('body'), readFont('h1'), readFont('.page-title, h2')].filter(Boolean),
      regions: {
        appShell: visible('nav.navigation') || visible('header'),
        setupScreen: visible('#setup, [data-testid="setup-screen"], form'),
        loginScreen: visible('form'),
        main: visible('main'),
      },
      interactiveCount: interactive.length,
      smallTargets: smallTargets.slice(0, 10),
      smallTargetCount: smallTargets.length,
      title: document.title,
      lang: de.getAttribute('lang'),
      dir: de.getAttribute('dir'),
    };
  });

  const screenshotPath = path.join(OUT, `dpi-${String(SCALE).replace('.', '_')}.png`);
  await page.screenshot({ path: screenshotPath });

  const checks = [
    {
      id: 'DPI-01',
      name: 'devicePixelRatio يطابق مقياس الجهاز المطلوب',
      ok: Math.abs(measurements.devicePixelRatio - SCALE) < 0.02,
      detail: `dpr=${measurements.devicePixelRatio} expected=${SCALE}`,
    },
    {
      id: 'DPI-02',
      name: 'لا تجاوز أفقي على مستوى المستند (لا شريط تمرير أفقي غير مقصود)',
      ok: measurements.document.horizontalOverflow <= 1,
      detail: `scrollWidth=${measurements.document.scrollWidth} clientWidth=${measurements.document.clientWidth}`,
    },
    {
      id: 'DPI-03',
      name: 'لا عنصر مرئي يتجاوز عرض النافذة',
      ok: measurements.overflowingCount === 0,
      detail: `${measurements.overflowingCount} عنصر متجاوز`,
    },
    {
      id: 'DPI-04',
      name: 'قشرة التطبيق/شاشة الدخول مرئية وقابلة للقياس',
      ok: measurements.regions.appShell || measurements.regions.loginScreen || measurements.regions.main,
      detail: JSON.stringify(measurements.regions),
    },
    {
      id: 'DPI-05',
      name: 'نقاط النقر لا تقل عن 24px عند هذا التكبير',
      ok: measurements.smallTargetCount === 0,
      detail: `${measurements.smallTargetCount} عنصر أصغر من 24px`,
    },
  ];

  const report = {
    harness: 'nexus-dpi-cdp',
    measuredAt: new Date().toISOString(),
    scale: SCALE,
    browser: version.Browser,
    userAgent: version['User-Agent'],
    measurements,
    checks,
    passed: checks.filter((c) => c.ok).length,
    total: checks.length,
    verdict: checks.every((c) => c.ok) ? 'PASS' : 'FAIL',
    screenshotPath,
  };

  writeFileSync(path.join(OUT, `dpi-${String(SCALE).replace('.', '_')}.json`), JSON.stringify(report, null, 2));
  for (const c of checks) console.log(`[${c.ok ? 'PASS' : 'FAIL'}] ${c.id} — ${c.name} (${c.detail})`);
  console.log(`DPI_SUMMARY scale=${SCALE} passed=${report.passed}/${report.total} verdict=${report.verdict}`);
  await browser.close();
  process.exit(report.verdict === 'PASS' ? 0 : 1);
};

main().catch((error) => {
  console.error(`DPI_FAILURE: ${error.message}`);
  process.exit(2);
});
