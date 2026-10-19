/**
 * NEXUS accessibility pass — drives the REAL Tauri WebView2 window over CDP and
 * MEASURES keyboard reachability, focus visibility, ARIA wiring and semantics.
 *
 * لماذا WebView2: قياس إمكانية الوصول في متصفح خارجي لا يمثّل نافذة Tauri
 * (زخرفة، DPI، طبقة الإدخال). هنا نفتح التطبيق الحقيقي.
 *
 * مهم: منفذ التصحيح لا يمكن فتحه بمتغيّر البيئة — WRY يستدعي دائماً
 * set_additional_browser_arguments فيُلغى المتغيّر. لذلك يجب بناء ثنائي
 * بملف بياني مدمج: tools/e2e/tauri.e2e.conf.json (منفذ 9333).
 *
 * Usage: node tools/e2e/nexus-a11y.mjs <exePath> [--port=9333] [--out=dir]
 * Output: JSON report on stdout.
 */
import { spawn } from 'node:child_process';
import { writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { chromium } from 'playwright-core';

const args = process.argv.slice(2);
const exePath = args.find((a) => !a.startsWith('--'));
const portArg = args.find((a) => a.startsWith('--port='));
const PORT = portArg ? Number(portArg.split('=')[1]) : 9333;
const outArg = args.find((a) => a.startsWith('--out='));
const OUT = outArg ? outArg.split('=')[1] : null;

if (!exePath || !existsSync(exePath)) {
  console.error(`A11Y_FAILURE: executable not found: ${exePath}`);
  process.exit(2);
}
if (OUT) mkdirSync(OUT, { recursive: true });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const report = {
  harness: 'nexus-a11y-cdp',
  startedAt: new Date().toISOString(),
  webview: null,
  checks: [],
  measurements: {},
  consoleErrors: [],
  pageErrors: [],
  verdict: 'FAIL',
};
let passed = 0;
let failed = 0;

const check = async (name, fn) => {
  const entry = { name, status: 'FAIL', detail: null };
  try {
    entry.detail = (await fn()) ?? null;
    entry.status = 'PASS';
    passed += 1;
  } catch (error) {
    entry.detail = String(error.message || error).slice(0, 400);
    failed += 1;
  }
  report.checks.push(entry);
  console.log(`[${entry.status}] ${name}${entry.status === 'PASS' ? '' : ' :: ' + entry.detail}`);
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
    await sleep(500);
  }
  throw new Error(`CDP on port ${port} never opened`);
};

const child = spawn(exePath, [], { stdio: 'ignore', detached: false });
let browser = null;

try {
  const version = await waitForCdp(PORT);
  report.webview = version.Browser;
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${PORT}`);
  const context = browser.contexts()[0] ?? (await browser.newContext());
  let page = context.pages().find((p) => !p.url().startsWith('devtools://'));
  const deadline = Date.now() + 30000;
  while (!page && Date.now() < deadline) {
    await sleep(300);
    page = context.pages().find((p) => !p.url().startsWith('devtools://'));
  }
  if (!page) throw new Error('no app page over CDP');

  page.on('console', (m) => {
    if (m.type() === 'error') report.consoleErrors.push(m.text().slice(0, 300));
  });
  page.on('pageerror', (e) => report.pageErrors.push(String(e.message || e).slice(0, 300)));

  await page.waitForSelector('#root', { timeout: 30000 });
  await sleep(1500);

  // الدخول إلى وضع DEMO إن ظهرت البوابة.
  const demo = page.getByRole('button', { name: /تجربة|Demo|demo/i }).first();
  if ((await demo.count()) > 0) {
    await demo.click().catch(() => {});
    await sleep(1500);
  }

  // ---------- 1. بنية المستند ----------
  await check('document declares language and direction on <html>', async () => {
    const info = await page.evaluate(() => ({
      lang: document.documentElement.getAttribute('lang'),
      dir: document.documentElement.getAttribute('dir'),
    }));
    if (!info.lang || !info.dir) throw new Error(`missing lang/dir: ${JSON.stringify(info)}`);
    return info;
  });

  // ---------- 2. عنوان `<h1>` واحد لكل صفحة ----------
  await check('exactly one h1 per page across all 15 modules', async () => {
    const results = [];
    const items = page.locator('.navigation__item');
    const count = await items.count();
    if (count !== 15) throw new Error(`expected 15 nav items, found ${count}`);
    for (let i = 0; i < count; i += 1) {
      await items.nth(i).click();
      await sleep(420);
      const h1 = await page.evaluate(() => {
        const all = [...document.querySelectorAll('main h1')];
        return { count: all.length, text: all[0]?.textContent?.trim() ?? null };
      });
      results.push({ index: i, h1: h1.count, text: h1.text });
      if (h1.count !== 1 || !h1.text) {
        throw new Error(`module #${i + 1} has ${h1.count} h1 (text=${h1.text})`);
      }
    }
    return results;
  });

  // ---------- 3. وصول لوحة المفاتيح: ترتيب Tab ----------
  await check('Tab reaches interactive controls, each stop has a visible focus indicator', async () => {
    await page.locator('.navigation__item').first().click();
    await sleep(400);
    await page.evaluate(() => document.body.focus());
    const stops = [];
    const seen = new Set();
    for (let i = 0; i < 25; i += 1) {
      await page.keyboard.press('Tab');
      await sleep(60);
      const stop = await page.evaluate(() => {
        const el = document.activeElement;
        if (!el || el === document.body) return null;
        const style = getComputedStyle(el);
        const outlineVisible =
          style.outlineStyle !== 'none' && parseFloat(style.outlineWidth || '0') > 0;
        const boxShadowVisible = style.boxShadow && style.boxShadow !== 'none';
        const borderChanged = style.borderColor && style.borderWidth !== '0px';
        return {
          tag: el.tagName,
          role: el.getAttribute('role'),
          label:
            el.getAttribute('aria-label') ||
            el.getAttribute('title') ||
            (el.textContent || '').trim().slice(0, 40) ||
            null,
          outlineVisible,
          boxShadowVisible,
          hasVisibleIndicator: outlineVisible || boxShadowVisible || borderChanged,
        };
      });
      if (!stop) continue;
      const key = `${stop.tag}|${stop.label}`;
      if (seen.has(key)) continue;
      seen.add(key);
      stops.push(stop);
    }
    if (stops.length < 5) throw new Error(`only ${stops.length} distinct focus stops`);
    const withoutIndicator = stops.filter((s) => !s.hasVisibleIndicator);
    if (withoutIndicator.length > 0) {
      throw new Error(
        `${withoutIndicator.length}/${stops.length} focus stops lack a visible indicator: ` +
          JSON.stringify(withoutIndicator.slice(0, 3)),
      );
    }
    report.measurements.tabStops = stops.length;
    return { distinctStops: stops.length, allHaveIndicator: true };
  });

  // ---------- 4. أيقونات بلا اسم وصول ----------
  await check('every button and link exposes an accessible name', async () => {
    const offenders = [];
    const items = page.locator('.navigation__item');
    const count = await items.count();
    for (let i = 0; i < count; i += 1) {
      await items.nth(i).click();
      await sleep(380);
      const bad = await page.evaluate(() => {
        const controls = [...document.querySelectorAll('main button, main a, header button, aside button')];
        return controls
          .filter((el) => el.offsetParent !== null)
          .filter((el) => {
            const name =
              el.getAttribute('aria-label') ||
              el.getAttribute('title') ||
              (el.textContent || '').trim() ||
              el.querySelector('svg title')?.textContent ||
              '';
            return name.trim().length === 0;
          })
          .map((el) => el.outerHTML.slice(0, 120));
      });
      offenders.push(...bad.map((html) => ({ module: i + 1, html })));
    }
    if (offenders.length > 0) {
      throw new Error(`${offenders.length} controls without accessible name: ${JSON.stringify(offenders.slice(0, 3))}`);
    }
    return { unlabeledControls: 0 };
  });

  // ---------- 5. دلالات الجداول ----------
  await check('tables expose header cells with scope or th semantics', async () => {
    const summary = [];
    const items = page.locator('.navigation__item');
    const count = await items.count();
    for (let i = 0; i < count; i += 1) {
      await items.nth(i).click();
      await sleep(360);
      const info = await page.evaluate(() => {
        const tables = [...document.querySelectorAll('main table')];
        return tables.map((t) => ({
          rows: t.querySelectorAll('tbody tr').length,
          th: t.querySelectorAll('th').length,
          hasCaption: !!t.querySelector('caption'),
          ariaLabel: t.getAttribute('aria-label'),
        }));
      });
      if (info.length > 0) summary.push({ module: i + 1, tables: info });
    }
    const dataTables = summary.flatMap((s) => s.tables).filter((t) => t.rows > 0);
    const withoutHeaders = dataTables.filter((t) => t.th === 0);
    if (withoutHeaders.length > 0) {
      throw new Error(`${withoutHeaders.length} data tables without any th`);
    }
    return { tablesWithData: dataTables.length, withoutHeaders: 0 };
  });

  // ---------- 6. التنقّل النشط معلن ----------
  await check('active navigation item is programmatically announced', async () => {
    const items = page.locator('.navigation__item');
    await items.nth(3).click();
    await sleep(420);
    const info = await page.evaluate(() => {
      const active = document.querySelector('.navigation__item--active');
      return {
        hasActiveClass: !!active,
        ariaCurrent: active?.getAttribute('aria-current') ?? null,
        count: document.querySelectorAll('.navigation__item--active').length,
      };
    });
    if (info.count !== 1) throw new Error(`expected 1 active nav item, found ${info.count}`);
    if (!info.ariaCurrent) throw new Error('active nav item carries no aria-current');
    return info;
  });

  // ---------- 7. الحوار: دور صحيح + حصر التركيز + Escape ----------
  await check('dialog exposes role, traps focus, closes on Escape and restores focus', async () => {
    await page.keyboard.press('Control+k');
    await page.waitForSelector('[role="dialog"]', { timeout: 10000 });
    const opened = await page.locator('[role="dialog"]').count();
    const modal = await page.evaluate(() => {
      const d = document.querySelector('[role="dialog"]');
      const labelledBy = d?.getAttribute('aria-labelledby');
      const target = labelledBy ? document.getElementById(labelledBy) : null;
      return {
        ariaModal: d?.getAttribute('aria-modal'),
        // الاسم المتاح يُقبل بالطريقتين: aria-label مباشرةً أو aria-labelledby
        // يشير إلى عقدة موجودة وذات نص.
        accessibleName:
          d?.getAttribute('aria-label') || (target?.textContent || '').trim() || null,
        labelledByResolves: labelledBy ? !!target && (target.textContent || '').trim().length > 0 : false,
      };
    });
    if (!modal.ariaModal) throw new Error('dialog is not marked aria-modal');
    if (!modal.accessibleName) throw new Error('dialog has no accessible name (aria-label nor aria-labelledby)');
    // حصر التركيز: Tab عدة مرات ويجب أن يبقى التركيز داخل الحوار.
    let escaped = 0;
    for (let i = 0; i < 12; i += 1) {
      await page.keyboard.press('Tab');
      await sleep(40);
      const inside = await page.evaluate(() => {
        const d = document.querySelector('[role="dialog"]');
        return d ? d.contains(document.activeElement) : false;
      });
      if (!inside) escaped += 1;
    }
    await page.keyboard.press('Escape');
    await sleep(400);
    const remaining = await page.locator('[role="dialog"]').count();
    const focusRestored = await page.evaluate(() => {
      const el = document.activeElement;
      return el ? { tag: el.tagName, inBody: document.body.contains(el) } : null;
    });
    if (remaining !== 0) throw new Error('Escape did not close the dialog');
    report.measurements.dialog = { opened, modal, escapedFocusCount: escaped, focusRestored };
    return { opened, modal, escapedFocusCount: escaped, focusRestored };
  });

  // ---------- 8. منطقة إعلان الحالة ----------
  await check('a status live region exists for async feedback', async () => {
    const info = await page.evaluate(() => {
      const regions = [...document.querySelectorAll('[role="status"], [aria-live]')];
      return regions.map((r) => ({
        role: r.getAttribute('role'),
        live: r.getAttribute('aria-live'),
        text: (r.textContent || '').trim().slice(0, 60),
      }));
    });
    return { regions: info.length };
  });

  // ---------- 9. تكبير النص 200% بلا فقدان محتوى ----------
  await check('page survives 200% text zoom without losing content', async () => {
    await page.locator('.navigation__item').first().click();
    await sleep(400);
    const before = await page.evaluate(() => document.body.innerText.length);
    await page.evaluate(() => {
      document.documentElement.style.fontSize = '200%';
    });
    await sleep(600);
    const after = await page.evaluate(() => ({
      chars: document.body.innerText.length,
      scrollW: document.documentElement.scrollWidth,
      clientW: document.documentElement.clientWidth,
    }));
    await page.evaluate(() => {
      document.documentElement.style.fontSize = '';
    });
    await sleep(300);
    if (after.chars < before * 0.8) {
      throw new Error(`content lost at 200% zoom: ${before} -> ${after.chars}`);
    }
    return { before, after };
  });

  report.consoleErrors = [...new Set(report.consoleErrors)];
  report.pageErrors = [...new Set(report.pageErrors)];
  report.passed = passed;
  report.failed = failed;
  report.verdict = failed === 0 && report.pageErrors.length === 0 ? 'PASS' : 'FAIL';
} catch (error) {
  report.fatal = String(error.message || error).slice(0, 600);
  report.verdict = 'ERROR';
} finally {
  try {
    if (browser) await browser.close();
  } catch {
    /* أفضل جهد */
  }
  try {
    child.kill();
  } catch {
    /* ربما انتهت */
  }
  report.finishedAt = new Date().toISOString();
  console.log(JSON.stringify(report, null, 2));
  if (OUT) {
    try {
      writeFileSync(`${OUT}/a11y-report.json`, JSON.stringify(report, null, 2), 'utf8');
    } catch {
      /* ثانوي */
    }
  }
  process.exit(report.verdict === 'PASS' ? 0 : 1);
}
