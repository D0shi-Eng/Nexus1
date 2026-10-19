/**
 * NEXUS V1 — UI / i18n / THEMES / ACCESSIBILITY PASS (REAL WEBVIEW2)
 * ----------------------------------------------------------------
 * يُشغَّل على قاعدة الديمو المعزولة بعد جولة الأعمال، ويقيس داخل نافذة
 * WebView2 الفعلية: اتجاه اللغة، الثيمات السبعة، تباين النص، ترتيب التركيب
 * بلوحة المفاتيح، حصر التركيز في الحوار، وأسماء الوصول لكل وحدة.
 *
 * Usage: node nexus-uat-ui.mjs <exePath> --evidence=<dir> [--port=9444]
 */
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { chromium } from 'playwright-core';

const argv = process.argv.slice(2);
const exePath = argv.find((a) => !a.startsWith('--'));
const argOf = (name, fallback) => {
  const found = argv.find((a) => a.startsWith(`--${name}=`));
  return found ? found.slice(name.length + 3) : fallback;
};
const PORT = Number(argOf('port', '9444'));
const EVIDENCE = argOf('evidence', null);
if (!exePath || !existsSync(exePath)) {
  console.error(`UI_PASS_FAILURE: executable not found: ${exePath}`);
  process.exit(2);
}
if (EVIDENCE) mkdirSync(EVIDENCE, { recursive: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const report = { harness: 'nexus-uat-ui', startedAt: new Date().toISOString(), steps: [], measurements: {}, summary: { total: 0, passed: 0, failed: 0 }, consoleErrors: [], pageErrors: [] };
const step = async (name, fn) => {
  report.summary.total += 1;
  const entry = { name, status: 'FAIL', detail: null };
  try {
    entry.detail = (await fn()) ?? null;
    entry.status = 'PASS';
    report.summary.passed += 1;
  } catch (error) {
    entry.detail = String(error?.message ?? error).slice(0, 500);
    report.summary.failed += 1;
  }
  report.steps.push(entry);
  console.log(`[${entry.status}] ${name}${entry.status === 'PASS' ? '' : ' :: ' + entry.detail}`);
};
const assert = (condition, message) => { if (!condition) throw new Error(message); };

const child = spawn(exePath, [], { stdio: 'ignore' });
let browser = null;
let page = null;

const shot = async (name) => {
  if (!EVIDENCE) return;
  await page.screenshot({ path: path.join(EVIDENCE, `${name}.png`) }).catch(() => {});
};

const contrastRatio = () => page.evaluate(() => {
  const parse = (value) => {
    const match = value.match(/rgba?\(([^)]+)\)/);
    if (!match) return null;
    const parts = match[1].split(',').map((item) => Number(item.trim()));
    return { r: parts[0], g: parts[1], b: parts[2], a: parts.length > 3 ? parts[3] : 1 };
  };
  const luminance = (color) => {
    const channel = (value) => {
      const normalized = value / 255;
      return normalized <= 0.03928 ? normalized / 12.92 : ((normalized + 0.055) / 1.055) ** 2.4;
    };
    return 0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b);
  };
  const textColor = parse(getComputedStyle(document.body).color);
  // الخلفية قد تُرسم على body أو .app-shell بدل html — نأخذ أول طبقة غير شفافة.
  let backgroundColor = null;
  let backgroundSource = '';
  for (const [source, element] of [['html', document.documentElement], ['body', document.body], ['app-shell', document.querySelector('.app-shell')]]) {
    if (!element) continue;
    const parsed = parse(getComputedStyle(element).backgroundColor);
    if (parsed && parsed.a > 0.5) { backgroundColor = parsed; backgroundSource = source; break; }
  }
  if (!textColor || !backgroundColor) return null;
  const lighter = Math.max(luminance(textColor), luminance(backgroundColor));
  const darker = Math.min(luminance(textColor), luminance(backgroundColor));
  return {
    ratio: Number(((lighter + 0.05) / (darker + 0.05)).toFixed(2)),
    text: getComputedStyle(document.body).color,
    resolvedBackground: `rgb(${backgroundColor.r}, ${backgroundColor.g}, ${backgroundColor.b})`,
    backgroundSource,
  };
});

try {
  const deadline = Date.now() + 90000;
  let ready = false;
  while (!ready && Date.now() < deadline) {
    try { const res = await fetch(`http://127.0.0.1:${PORT}/json/version`); ready = res.ok; } catch { /* retry */ }
    if (!ready) await sleep(400);
  }
  if (!ready) throw new Error('CDP endpoint never opened');
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${PORT}`);
  const context = browser.contexts()[0];
  let found = context.pages().find((p) => !p.url().startsWith('devtools://'));
  const pageDeadline = Date.now() + 40000;
  while (!found && Date.now() < pageDeadline) {
    await sleep(300);
    found = context.pages().find((p) => !p.url().startsWith('devtools://'));
  }
  if (!found) throw new Error('no application page found');
  page = found;
  page.on('console', (message) => { if (message.type() === 'error') report.consoleErrors.push(message.text().slice(0, 300)); });
  page.on('pageerror', (error) => report.pageErrors.push(String(error?.message ?? error).slice(0, 300)));
  await page.waitForSelector('#root', { timeout: 60000 });
  await page.waitForSelector('nav.navigation', { timeout: 60000 });

  const modules = ['نظرة تنفيذية', 'ما يحتاج إلى اهتمام', 'الالتزامات', 'الذكاء التنظيمي', 'المخاطر', 'الأدلة', 'المهام', 'الضوابط', 'الوكلاء', 'المعرفة', 'التقارير', 'التدقيق', 'المؤسسة', 'التكاملات', 'الإعدادات'];
  const navTo = async (name) => {
    if ((await page.locator('[role="dialog"]').count()) > 0) await page.keyboard.press('Escape');
    await page.locator('nav.navigation button', { hasText: name }).first().click({ timeout: 20000 });
    await sleep(450);
  };

  await step('U1 — العربية RTL هي الاتجاه الافتراضي', async () => {
    const state = await page.evaluate(() => ({ lang: document.documentElement.lang, dir: document.documentElement.dir }));
    assert(state.lang === 'ar', `default lang must be ar, got ${state.lang}`);
    assert(state.dir === 'rtl', `default dir must be rtl, got ${state.dir}`);
    const nav = await page.locator('nav.navigation button').allTextContents();
    assert(nav.length === 15, `expected 15 nav items, got ${nav.length}`);
    await shot('ui-01-ar-rtl');
    return state;
  });

  await step('U2 — التبديل إلى الإنجليزية LTR يقلب الاتجاه والتنقل دون كسر', async () => {
    await page.locator('.language-button').first().click();
    await sleep(700);
    const state = await page.evaluate(() => ({ lang: document.documentElement.lang, dir: document.documentElement.dir }));
    const nav = await page.locator('nav.navigation button').allTextContents();
    assert(state.lang === 'en', `lang after switch must be en, got ${state.lang}`);
    assert(state.dir === 'ltr', `dir after switch must be ltr, got ${state.dir}`);
    assert(nav[0] === 'Executive Overview', `navigation did not localise, first item: ${nav[0]}`);
    const bodyLength = await page.evaluate(() => (document.querySelector('main')?.textContent ?? '').trim().length);
    assert(bodyLength > 200, 'English UI rendered empty content');
    await shot('ui-02-en-ltr');
    await page.locator('.language-button').first().click();
    await sleep(700);
    const restored = await page.evaluate(() => document.documentElement.dir);
    assert(restored === 'rtl', `language switch back failed, dir=${restored}`);
    return { english: state, restoredDir: restored, navCount: nav.length };
  });

  await step('U3 — الثيمات الستة + اتباع النظام: تطبيق فعلي وتباين مقيس', async () => {
    await navTo('الإعدادات');
    // قسم المظهر داخل تنقل الإعدادات الفرعي — لا يُعرض افتراضياً.
    await page.locator('.settings-nav__item', { hasText: 'المظهر' }).first().click({ timeout: 15000 });
    await sleep(500);
    const preferences = ['light', 'dark', 'midnight', 'sand', 'emerald', 'contrast', 'system'];
    const results = [];
    for (const preference of preferences) {
      const radio = page.locator(`input[name="nexus-theme"][value="${preference}"]`).first();
      await radio.waitFor({ state: 'attached', timeout: 15000 });
      await radio.check({ force: true });
      await sleep(650);
      const measured = await page.evaluate(() => ({
        dataTheme: document.documentElement.getAttribute('data-theme'),
        persisted: window.localStorage.getItem('nexus.theme'),
        overflow: document.scrollingElement.scrollWidth - document.scrollingElement.clientWidth,
        googleFont: getComputedStyle(document.body).fontFamily.slice(0, 90),
      }));
      const contrast = await contrastRatio();
      results.push({ preference, ...measured, contrast });
      await shot(`ui-theme-${preference}`);
      assert(measured.persisted === preference, `theme preference not persisted for ${preference}`);
      assert(measured.overflow <= 1, `horizontal overflow ${measured.overflow}px under theme ${preference}`);
      if (preference !== 'system') {
        assert(measured.dataTheme === preference, `data-theme expected ${preference}, got ${measured.dataTheme}`);
      } else {
        assert(measured.dataTheme === null, `system preference must not force data-theme, got ${measured.dataTheme}`);
      }
      assert(contrast && contrast.ratio >= 4.5, `contrast ${contrast?.ratio} below 4.5 under theme ${preference}`);
    }
    const distinctBackgrounds = new Set(results.map((item) => item.contrast.resolvedBackground));
    assert(distinctBackgrounds.size >= 5, `themes must produce distinct palettes, got ${distinctBackgrounds.size}`);
    report.measurements.themes = results;
    // استعادة الثيم الافتراضي «النظام» كما يبدأ به المنتج.
    await page.locator('input[name="nexus-theme"][value="system"]').first().check({ force: true });
    await sleep(400);
    return results.map((item) => ({ preference: item.preference, dataTheme: item.dataTheme, ratio: item.contrast.ratio, overflow: item.overflow }));
  });

  await step('U4 — ترتيب التركيب بلوحة المفاتيح ومؤشر تركيز مرئي', async () => {
    await page.evaluate(() => document.body.focus());
    await page.locator('nav.navigation button').first().focus();
    const samples = [];
    for (let index = 0; index < 20; index += 1) {
      await page.keyboard.press('Tab');
      const sample = await page.evaluate(() => {
        const active = document.activeElement;
        if (!active) return null;
        const style = getComputedStyle(active);
        return {
          tag: active.tagName,
          text: (active.getAttribute('aria-label') ?? active.textContent ?? '').trim().slice(0, 40),
          outlineWidth: style.outlineWidth,
          outlineStyle: style.outlineStyle,
          boxShadow: style.boxShadow,
        };
      });
      if (sample) samples.push(sample);
    }
    assert(samples.length === 20, `only ${samples.length} focus stops measured`);
    const invisible = samples.filter((sample) => (sample.outlineWidth === '0px' || sample.outlineStyle === 'none') && sample.boxShadow === 'none');
    assert(invisible.length === 0, `focuses without a visible indicator: ${JSON.stringify(invisible.slice(0, 3))}`);
    const uniqueStops = new Set(samples.map((sample) => `${sample.tag}:${sample.text}`));
    assert(uniqueStops.size >= 10, `keyboard order only reaches ${uniqueStops.size} distinct controls`);
    report.measurements.keyboard = { stops: samples.length, distinct: uniqueStops.size, samples };
    return { stops: samples.length, distinct: uniqueStops.size };
  });

  await step('U5 — حصر التركيز داخل الحوار والإغلاق بـEscape واستعادة التركيز', async () => {
    const searchButton = page.locator('.topbar__search').first();
    await searchButton.focus();
    await page.keyboard.press('Control+k');
    await page.waitForSelector('[role="dialog"]', { timeout: 15000 });
    const dialog = page.locator('[role="dialog"]').first();
    const labelled = await page.evaluate(() => {
      const node = document.querySelector('[role="dialog"]');
      if (!node) return false;
      const byLabel = node.getAttribute('aria-label');
      const byLabelledBy = node.getAttribute('aria-labelledby');
      const resolved = byLabelledBy ? document.getElementById(byLabelledBy)?.textContent?.trim() : null;
      return Boolean((byLabel && byLabel.length > 0) || (resolved && resolved.length > 0));
    });
    assert(labelled, 'dialog has no accessible name');
    let escapes = 0;
    for (let index = 0; index < 12; index += 1) {
      await page.keyboard.press('Tab');
      const inside = await page.evaluate(() => {
        const node = document.querySelector('[role="dialog"]');
        return Boolean(node && document.activeElement && node.contains(document.activeElement));
      });
      if (!inside) escapes += 1;
    }
    assert(escapes === 0, `focus escaped the modal ${escapes} times`);
    await page.keyboard.press('Escape');
    await sleep(400);
    const closed = (await page.locator('[role="dialog"]').count()) === 0;
    const focusRestored = await page.evaluate(() => document.activeElement?.className ?? '');
    assert(closed, 'dialog did not close on Escape');
    assert(String(focusRestored).includes('topbar__search'), `focus not restored to the trigger, active: ${focusRestored}`);
    await shot('ui-03-focus-trap');
    return { ariaModal: await page.getAttribute('[role="dialog"]', 'aria-modal').catch(() => null), escapes, closed, focusRestored: true, dialogExistsAfter: await dialog.count() };
  });

  await step('U6 — الوحدات الـ15: عنوان واحد، أسماء وصول، جداول بترويسة، وحدة نشطة واحدة', async () => {
    const findings = [];
    for (const name of modules) {
      await navTo(name);
      const measured = await page.evaluate(() => {
        const main = document.querySelector('main');
        const h1 = main?.querySelectorAll('h1') ?? [];
        const interactive = [...(main?.querySelectorAll('button, a[href], input, select, textarea') ?? [])];
        const unnamed = interactive.filter((element) => {
          const text = (element.textContent ?? '').trim();
          const aria = element.getAttribute('aria-label') ?? '';
          const labelledBy = element.getAttribute('aria-labelledby');
          const resolved = labelledBy ? document.getElementById(labelledBy)?.textContent?.trim() : '';
          const title = element.getAttribute('title') ?? '';
          const associated = element.id ? document.querySelector(`label[for="${element.id}"]`)?.textContent?.trim() : '';
          const wrapped = element.closest('label')?.textContent?.trim() ?? '';
          return !(text || aria || resolved || title || associated || wrapped);
        }).map((element) => element.tagName);
        const tables = [...(main?.querySelectorAll('table') ?? [])];
        const tablesWithoutHeader = tables.filter((table) => table.querySelectorAll('th').length === 0).length;
        return {
          h1Count: h1.length,
          unnamedControls: unnamed.length,
          tables: tables.length,
          tablesWithoutHeader,
          activeNav: document.querySelectorAll('nav.navigation button[aria-current="page"]').length,
        };
      });
      findings.push({ module: name, ...measured });
      assert(measured.h1Count === 1, `${name}: expected exactly one h1, got ${measured.h1Count}`);
      assert(measured.unnamedControls === 0, `${name}: ${measured.unnamedControls} controls without an accessible name`);
      assert(measured.tablesWithoutHeader === 0, `${name}: ${measured.tablesWithoutHeader} tables without headers`);
      assert(measured.activeNav === 1, `${name}: ${measured.activeNav} active nav items`);
    }
    report.measurements.modules = findings;
    await shot('ui-04-modules-a11y');
    return { modules: findings.length, allClean: true };
  });

  await step('U7 — لا أخطاء صفحة أو console خلال جولة الواجهة', async () => {
    assert(report.pageErrors.length === 0, `page errors: ${report.pageErrors.join(' ; ')}`);
    assert(report.consoleErrors.length === 0, `console errors: ${report.consoleErrors.join(' ; ')}`);
    return { pageErrors: 0, consoleErrors: 0 };
  });
} catch (error) {
  report.fatal = String(error?.message ?? error);
  console.error(`UI_PASS_FATAL: ${report.fatal}`);
} finally {
  report.finishedAt = new Date().toISOString();
  if (EVIDENCE) writeFileSync(path.join(EVIDENCE, 'uat-ui-report.json'), JSON.stringify(report, null, 2), 'utf8');
  if (browser) await browser.close().catch(() => {});
  try { child.kill(); } catch { /* already gone */ }
  console.log(`\nUI_SUMMARY total=${report.summary.total} passed=${report.summary.passed} failed=${report.summary.failed}`);
  process.exit(report.summary.failed === 0 && !report.fatal ? 0 : 1);
}
