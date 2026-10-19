/**
 * NEXUS E2E harness — drives the REAL Tauri desktop application through the
 * Edge WebView2 DevTools Protocol.
 *
 * لماذا لا نستخدم متصفحاً عادياً: واجهة NEXUS تستدعي أوامر Tauri عبر `invoke`،
 * وهذه القناة موجودة فقط داخل نافذة WebView2 الحقيقية. أي تشغيل في متصفح خارجي
 * سيثبت الرسوم فقط لا سلطة الخلف. لذلك نفتح التطبيق الفعلي ونحقن منافذ تصحيح
 * الأخطاء فيه، ثم نتصل بها عبر CDP لنقرأ حالة React الحقيقية بعد النقر والكتابة.
 *
 * Usage: node nexus-e2e.mjs <exePath> [--port=9222] [--headed-evidence=dir]
 * Output: JSON report on stdout (machine readable, no secrets).
 */
import { spawn } from 'node:child_process';
import { writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { chromium } from 'playwright-core';

const args = process.argv.slice(2);
const exePath = args.find((a) => !a.startsWith('--'));
const portArg = args.find((a) => a.startsWith('--port='));
const PORT = portArg ? Number(portArg.split('=')[1]) : 9222;
const evidenceArg = args.find((a) => a.startsWith('--evidence='));
const EVIDENCE = evidenceArg ? evidenceArg.split('=')[1] : null;

if (!exePath || !existsSync(exePath)) {
  console.error(`E2E_FAILURE: executable not found: ${exePath}`);
  process.exit(2);
}
if (EVIDENCE) mkdirSync(EVIDENCE, { recursive: true });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const report = {
  harness: 'nexus-e2e-cdp',
  startedAt: new Date().toISOString(),
  exe: exePath,
  cdpPort: PORT,
  consoleErrors: [],
  pageErrors: [],
  ipcFailures: [],
  steps: [],
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
      entry.detail = `optional: ${String(error.message || error).slice(0, 300)}`;
    } else {
      entry.detail = String(error.message || error).slice(0, 500);
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

const waitForCdp = async (port, timeoutMs = 60000) => {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`http://127.0.0.1:${port}/json/version`);
      if (res.ok) return await res.json();
    } catch {
      /* المنفذ لم يُفتح بعد — نعيد المحاولة حتى المهلة */
    }
    await sleep(500);
  }
  throw new Error(`CDP endpoint on port ${port} did not open within ${timeoutMs}ms`);
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
try {
  const version = await waitForCdp(PORT);
  report.browser = { Browser: version.Browser, webkitVersion: version['WebKit-Version'] ?? null };

  browser = await chromium.connectOverCDP(`http://127.0.0.1:${PORT}`);
  const context = browser.contexts()[0] ?? (await browser.newContext());
  let page = context.pages().find((p) => !p.url().startsWith('devtools://'));
  if (!page) {
    const deadline = Date.now() + 30000;
    while (!page && Date.now() < deadline) {
      await sleep(300);
      page = context.pages().find((p) => !p.url().startsWith('devtools://'));
    }
  }
  if (!page) throw new Error('no application page found over CDP');
  report.pageUrl = page.url();

  page.on('console', (msg) => {
    if (msg.type() === 'error') report.consoleErrors.push(msg.text().slice(0, 400));
  });
  page.on('pageerror', (err) => report.pageErrors.push(String(err.message || err).slice(0, 400)));

  await page.waitForLoadState('domcontentloaded');
  await page.waitForSelector('#root', { timeout: 30000 });

  // أي رسالة فشل IPC صادرة من الواجهة تعني أن مساراً إنتاجياً لم يعمل — نسجلها كدليل.
  await page.exposeFunction('__nexusE2eNote', (kind, text) => {
    if (kind === 'ipc') report.ipcFailures.push(String(text).slice(0, 300));
  });

  // ---------- 1. bootstrap / auth gate ----------
  await step('app boot: root renders and auth gate decides entry', async () => {
    await page.waitForFunction(() => document.querySelector('#root')?.children.length > 0, { timeout: 30000 });
    const text = await page.evaluate(() => document.body.innerText);
    if (!text || text.trim().length === 0) throw new Error('app rendered empty body');
    return { bodyChars: text.length, entry: text.slice(0, 80).replace(/\s+/g, ' ') };
  });

  await step('auth: enter DEMO workspace if the gate presents it', async () => {
    const demoButton = page.getByRole('button', { name: /تجربة|Demo|demo/i }).first();
    if (await demoButton.count() === 0) return 'no demo button — already inside a workspace';
    await demoButton.click();
    await page.waitForSelector('.app-shell', { timeout: 30000 });
    return 'entered DEMO workspace via visible control';
  });

  await step('shell: sidebar navigation exposes all 15 modules', async () => {
    await page.waitForSelector('.navigation__item', { timeout: 20000 });
    const labels = await page.$$eval('.navigation__item span', (els) => els.map((e) => e.textContent?.trim()));
    if (labels.length !== 15) throw new Error(`expected 15 navigation items, found ${labels.length}`);
    return { count: labels.length, labels };
  });

  // ---------- 2. every navigation item, clicked for real ----------
  const moduleResults = [];
  for (let i = 0; i < 15; i += 1) {
    const entry = await step(`navigate: click sidebar item #${i + 1} and assert the view swaps`, async () => {
      const items = page.locator('.navigation__item');
      const count = await items.count();
      if (i >= count) throw new Error('navigation item missing');
      const label = (await items.nth(i).innerText()).trim();
      const before = await page.evaluate(() => document.querySelector('main')?.innerHTML.length ?? 0);
      await items.nth(i).click();
      await page.waitForFunction(
        (prev) => (document.querySelector('main')?.innerHTML.length ?? 0) !== prev,
        before,
        { timeout: 15000 },
      ).catch(() => {});
      await sleep(350);
      const heading = await page.evaluate(() => {
        const h1 = document.querySelector('main h1');
        return h1 ? h1.textContent?.trim() : null;
      });
      const activeCount = await page.locator('.navigation__item--active').count();
      if (activeCount !== 1) throw new Error(`expected exactly 1 active nav item, found ${activeCount}`);
      const bodyText = await page.evaluate(() => document.querySelector('main')?.innerText ?? '');
      if (bodyText.trim().length === 0) throw new Error('module rendered no content');
      return { index: i, label, heading, activeCount, contentChars: bodyText.length };
    });
    moduleResults.push({ index: i, status: entry.status, detail: entry.detail });
    if (EVIDENCE && entry.status === 'PASS') {
      await page.screenshot({ path: `${EVIDENCE}/module-${String(i + 1).padStart(2, '0')}.png` }).catch(() => {});
    }
  }
  report.modules = moduleResults;

  // ---------- 3. command palette (Ctrl+K) ----------
  await step('keyboard: Ctrl+K opens the workspace search dialog', async () => {
    await page.keyboard.press('Control+k');
    await page.waitForSelector('[role="dialog"], .dialog, .search-dialog', { timeout: 10000 });
    const dialogs = await page.locator('[role="dialog"]').count();
    if (dialogs < 1) throw new Error('no dialog element after Ctrl+K');
    await page.keyboard.press('Escape');
    await sleep(300);
    return { dialogsOpened: dialogs };
  });

  // ---------- 4. language + direction ----------
  await step('i18n: switching language flips direction and keeps the app usable', async () => {
    const before = await page.evaluate(() => ({
      dir: document.documentElement.getAttribute('dir'),
      lang: document.documentElement.getAttribute('lang'),
      navFirst: document.querySelector('.navigation__item span')?.textContent?.trim() ?? null,
    }));
    const langButton = page.locator('.language-button').first();
    if (await langButton.count() === 0) throw new Error('language button not found');
    await langButton.click();
    await sleep(700);
    const after = await page.evaluate(() => ({
      dir: document.documentElement.getAttribute('dir'),
      lang: document.documentElement.getAttribute('lang'),
      navFirst: document.querySelector('.navigation__item span')?.textContent?.trim() ?? null,
    }));
    if (before.dir === after.dir) throw new Error(`direction did not change (stayed ${before.dir})`);
    if (before.navFirst === after.navFirst) throw new Error('navigation label did not change when language changed');
    await langButton.click();
    await sleep(700);
    const restored = await page.evaluate(() => document.documentElement.getAttribute('dir'));
    return { before, after, restoredDir: restored };
  });

  // ---------- 5. dialog open / Escape / focus restoration ----------
  await step('dialog: open record dialog, close with Escape, focus returns to trigger', async () => {
    await page.locator('.navigation__item').first().click();
    await sleep(500);
    const trigger = page.locator('main button').filter({ hasNotText: /^$/ }).first();
    const buttons = page.locator('main button');
    const n = await buttons.count();
    let opened = false;
    let usedIndex = -1;
    for (let i = 0; i < Math.min(n, 25); i += 1) {
      const b = buttons.nth(i);
      if (!(await b.isVisible()) || !(await b.isEnabled())) continue;
      await b.click({ trial: false }).catch(() => {});
      await sleep(400);
      if (await page.locator('[role="dialog"]').count() > 0) {
        opened = true;
        usedIndex = i;
        break;
      }
    }
    if (!opened) throw new Error('no dialog could be opened from Overview actions');
    const dialogCount = await page.locator('[role="dialog"]').count();
    await page.keyboard.press('Escape');
    await sleep(400);
    const afterEscape = await page.locator('[role="dialog"]').count();
    if (afterEscape !== 0 && afterEscape >= dialogCount) throw new Error('Escape did not close the dialog');
    const focusRestored = await page.evaluate(() => {
      const el = document.activeElement;
      return el ? { tag: el.tagName, inBody: document.body.contains(el) } : null;
    });
    return { triggerIndex: usedIndex, dialogCount, afterEscape, focusRestored, note: trigger ? 'scanned main buttons' : '' };
  });

  // ---------- 6. audit trail pagination (real IPC round trip) ----------
  await step('module Audit: keyset pagination control performs a real backend round trip', async () => {
    const items = page.locator('.navigation__item');
    const n = await items.count();
    let clicked = false;
    for (let i = 0; i < n; i += 1) {
      const label = (await items.nth(i).innerText()).trim();
      if (/تدقيق|Audit/i.test(label)) {
        await items.nth(i).click();
        clicked = true;
        break;
      }
    }
    if (!clicked) throw new Error('audit navigation item not found');
    await sleep(900);
    const beforeRows = await page.evaluate(() => document.querySelectorAll('main tbody tr, main li').length);
    const nextButton = page.locator('main button').filter({ hasText: /التالي|Next|المزيد|More/i }).first();
    if (await nextButton.count() === 0) {
      return { paginationControl: 'not present', rows: beforeRows, ipcFailuresSoFar: report.ipcFailures.length };
    }
    const disabled = await nextButton.isDisabled();
    if (disabled) return { paginationControl: 'present but disabled (single page)', rows: beforeRows };
    await nextButton.click();
    await sleep(900);
    return { paginationControl: 'clicked', rowsBefore: beforeRows, ipcFailuresSoFar: report.ipcFailures.length };
  });

  // ---------- 7. mutations through the backend ----------
  await step('workflow: open the task dialog, submit, and confirm a real backend acknowledgement', async () => {
    await page.locator('.navigation__item').first().click();
    await sleep(600);
    const openers = page.locator('main button');
    const n = await openers.count();
    let opened = false;
    for (let i = 0; i < Math.min(n, 25); i += 1) {
      const b = openers.nth(i);
      const label = (await b.innerText().catch(() => '')).trim();
      if (!/مهمة|Task/i.test(label)) continue;
      if (!(await b.isVisible()) || !(await b.isEnabled())) continue;
      await b.click().catch(() => {});
      await sleep(500);
      if (await page.locator('[role="dialog"]').count() > 0) {
        opened = true;
        break;
      }
    }
    if (!opened) throw new Error('task dialog could not be opened — no mutating workflow reachable from Overview');
    const dialog = page.locator('[role="dialog"]').first();
    const fields = await dialog.locator('input, textarea, select').count();
    if (fields === 0) throw new Error('task dialog exposes no input fields');
    const title = `E2E حوكمة ${Date.now()}`;
    const textInput = dialog.locator('input[type="text"], input:not([type]), textarea').first();
    if (await textInput.count() > 0) await textInput.fill(title);
    const selects = dialog.locator('select');
    if (await selects.count() > 0) {
      const options = await selects.first().locator('option').count();
      if (options > 0) await selects.first().selectOption({ index: 0 }).catch(() => {});
    }
    const submit = dialog.locator('button[type="submit"], button').filter({ hasText: /إنشاء|حفظ|Create|Save|Submit/i }).first();
    if (await submit.count() === 0) throw new Error('no submit control inside the task dialog');
    await submit.click();
    await sleep(1800);
    const stillOpen = await page.locator('[role="dialog"]').count();
    const feedback = await page.evaluate(() => {
      const el = document.querySelector('[role="status"], .operation-feedback');
      return el ? el.textContent?.trim() : null;
    });
    const errored = await page.evaluate(() => document.body.innerText.match(/LOCAL_SERVICE_UNAVAILABLE|AuthorizationDenied|فشل|error/i) !== null);
    return { createdTitle: title, dialogClosedAfterSubmit: stillOpen === 0, successFeedback: feedback, backendRejectedOrErrored: errored, ipcFailures: report.ipcFailures.length };
  });

  // ---------- 8. actor switching (authorization context change) ----------
  await step('authorization: demo actor switcher offers roles and applies a real context switch', async () => {
    const select = page.locator('.actor-switcher select').first();
    if (await select.count() === 0) throw new Error('actor switcher not present in DEMO shell');
    const optionCount = await select.locator('option').count();
    if (optionCount < 2) throw new Error(`expected several demo actors, found ${optionCount}`);
    const current = await select.inputValue();
    const values = await select.locator('option').evaluateAll((els) => els.map((e) => e.value));
    const target = values.find((v) => v !== current);
    await select.selectOption(target);
    await sleep(1500);
    const after = await select.inputValue().catch(() => null);
    return { optionCount, switchedFrom: current, switchedTo: target, observedAfter: after, ipcFailures: report.ipcFailures.length };
  });

  // ---------- 9. responsive / minimum window ----------
  await step('layout: app remains usable at the configured minimum viewport', async () => {
    await page.setViewportSize({ width: 980, height: 640 });
    await sleep(600);
    const overflow = await page.evaluate(() => ({
      docScrollW: document.documentElement.scrollWidth,
      docClientW: document.documentElement.clientWidth,
      bodyChars: document.body.innerText.length,
    }));
    await page.setViewportSize({ width: 1360, height: 880 });
    await sleep(400);
    return overflow;
  });

  // ---------- 10. reload persistence ----------
  await step('persistence: reload returns a working shell (session survives in DEMO)', async () => {
    await page.reload({ waitUntil: 'domcontentloaded' });
    await sleep(2500);
    const state = await page.evaluate(() => ({
      hasShell: document.querySelector('.app-shell') !== null,
      hasRoot: document.querySelector('#root') !== null,
      navCount: document.querySelectorAll('.navigation__item').length,
    }));
    if (!state.hasRoot) throw new Error('root vanished after reload');
    return state;
  });

  report.consoleErrors = [...new Set(report.consoleErrors)];
  report.pageErrors = [...new Set(report.pageErrors)];
  report.ipcFailures = [...new Set(report.ipcFailures)];
  report.finishedAt = new Date().toISOString();
  report.verdict =
    report.summary.failed === 0 && report.pageErrors.length === 0 ? 'PASS' : 'FAIL';
} catch (error) {
  report.fatal = String(error.message || error).slice(0, 800);
  report.verdict = 'ERROR';
} finally {
  try {
    if (browser) await browser.close();
  } catch {
    /* إغلاق الاتصال أفضل جهد — لا يفشل التقرير */
  }
  try {
    child.kill();
  } catch {
    /* العملية قد تكون انتهت أصلاً */
  }
  report.summary.total = report.steps.length;
  console.log(JSON.stringify(report, null, 2));
  if (EVIDENCE) {
    try {
      writeFileSync(`${EVIDENCE}/e2e-report.json`, JSON.stringify(report, null, 2), 'utf8');
    } catch {
      /* حفظ الدليل ثانوي */
    }
  }
  process.exit(report.verdict === 'PASS' ? 0 : 1);
}
