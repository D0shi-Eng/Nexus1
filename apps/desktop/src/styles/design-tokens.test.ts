import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

import { themeOrder } from '../lib/theme-order';

// حارس طبقة رموز التصميم.
//
// عيب مؤكَّد أُصلح (CQ-01): النسخة السابقة عرّفت 20 رمزاً بصيغة `--x: var(--x)`
// أي دورة على الذات — قيمة غير صالحة وقت الحساب — فسقطت 89 استخداماً لـ`var()`
// بلا قيمة احتياطية وصارت كل الحدود والأسطح والألوان الدلالية غير معرّفة.
//
// وبهذه الجولة صارت الثيمات ستّة. الخطر الجديد: ثيم يعلن مجموعة ناقصة فيرث قيمة
// ثيم آخر داخل المعاينة فيبدو خاطئاً بصمت. لذلك يُفرض هنا أن **كل ثيم يعلن
// المجموعة الدلالية الكاملة نفسها بالضبط**.

const readCss = (relativePath: string): string =>
  readFileSync(fileURLToPath(new URL(relativePath, import.meta.url)), 'utf8');

const stripComments = (css: string): string => css.replace(/\/\*[\s\S]*?\*\//g, '');

const TOKENS_CSS = stripComments(readCss('./tokens.css'));
const GLOBAL_CSS = stripComments(readCss('./global.css'));

type Declaration = { value: string; line: number };

const collectDeclarations = (css: string): Map<string, Declaration> => {
  const declarations = new Map<string, Declaration>();
  css.split(/\r?\n/).forEach((line, index) => {
    const pattern = /(--[A-Za-z0-9_-]+)\s*:\s*([^;{}]*)/g;
    let match: RegExpExecArray | null;
    while ((match = pattern.exec(line)) !== null) {
      declarations.set(match[1], { value: match[2].trim(), line: index + 1 });
    }
  });
  return declarations;
};

const collectReferences = (value: string): { name: string; hasFallback: boolean }[] => {
  const references: { name: string; hasFallback: boolean }[] = [];
  const pattern = /var\(\s*(--[A-Za-z0-9_-]+)\s*(,[^)]*)?\)/g;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(value)) !== null) {
    references.push({ name: match[1], hasFallback: Boolean(match[2]) });
  }
  return references;
};

/** استخراج كتلة CSS بالبحث عن أول قوس مطابق — كافٍ لملفاتنا المسطّحة. */
const extractBlock = (css: string, selector: string): string => {
  const start = css.indexOf(selector);
  if (start === -1) return '';
  const open = css.indexOf('{', start);
  if (open === -1) return '';
  let depth = 0;
  for (let index = open; index < css.length; index += 1) {
    if (css[index] === '{') depth += 1;
    else if (css[index] === '}') {
      depth -= 1;
      if (depth === 0) return css.slice(open + 1, index);
    }
  }
  return '';
};

const declarations = new Map([...collectDeclarations(TOKENS_CSS), ...collectDeclarations(GLOBAL_CSS)]);

const findCycles = (): string[] => {
  const cyclic: string[] = [];
  for (const [name, declaration] of declarations) {
    const seen = new Set([name]);
    let frontier = collectReferences(declaration.value).map((reference) => reference.name);
    let isCyclic = false;
    while (frontier.length > 0 && !isCyclic) {
      const next: string[] = [];
      for (const candidate of frontier) {
        if (candidate === name) {
          isCyclic = true;
          break;
        }
        if (seen.has(candidate)) continue;
        seen.add(candidate);
        const dependency = declarations.get(candidate);
        if (dependency) next.push(...collectReferences(dependency.value).map((reference) => reference.name));
      }
      frontier = next;
    }
    if (isCyclic) cyclic.push(name);
  }
  return cyclic;
};

// المجموعة الدلالية المرجعية: كل ما يعلنه الثيم الفاتح تحديداً في كتلته.
// الرموز البنيوية (المقاسات والمسافات والطباعة) تُعلن على :root مرة واحدة وليست
// جزءاً من المجموعة الدلالية، فيُستثنى بادئاتها.
const STRUCTURAL_PREFIX = /^--(space|radius|fs|lh|ls|weight|font|duration|easing|z|content|sidebar-width|topbar-height|focus-ring-width|focus-ring-offset|focus-ring$)/;

const lightBlock = extractBlock(TOKENS_CSS, "[data-theme='light'] {");
const canonicalThemeTokens = [...collectDeclarations(lightBlock).keys()]
  .filter((token) => !STRUCTURAL_PREFIX.test(token))
  .sort();

const themeBlocks = new Map<string, string>(
  themeOrder.map((id) => [id, extractBlock(TOKENS_CSS, `[data-theme='${id}'] {`)]),
);
const systemDarkBlock = extractBlock(TOKENS_CSS, ':root:not([data-theme])');

describe('طبقة رموز التصميم', () => {
  it('لا تحتوي أي رمز يشير إلى نفسه أو يدخل في دورة', () => {
    expect(findCycles()).toEqual([]);
  });

  it('كل استخدام لـ var() بلا قيمة احتياطية له تعريف فعلي', () => {
    const cycles = new Set(findCycles());
    const unresolved = new Set<string>();
    for (const css of [TOKENS_CSS, GLOBAL_CSS]) {
      for (const line of css.split(/\r?\n/)) {
        for (const reference of collectReferences(line)) {
          const declared = declarations.has(reference.name) && !cycles.has(reference.name);
          if (!declared && !reference.hasFallback) unresolved.add(reference.name);
        }
      }
    }
    expect([...unresolved]).toEqual([]);
  });

  it('الثيمات الستّة كلها لها كتلة دلالية في tokens.css', () => {
    expect(themeOrder).toEqual(['light', 'dark', 'midnight', 'sand', 'emerald', 'contrast']);
    for (const id of themeOrder) {
      expect(themeBlocks.get(id), `كتلة الثيم ${id}`).toBeTruthy();
    }
  });

  it('كل ثيم يعلن المجموعة الدلالية الكاملة نفسها — لا ثيم ناقص يرث من غيره', () => {
    expect(canonicalThemeTokens.length).toBeGreaterThan(40);
    const mismatches: string[] = [];
    for (const id of themeOrder) {
      const declared = [...collectDeclarations(themeBlocks.get(id) ?? '').keys()]
        .filter((token) => !STRUCTURAL_PREFIX.test(token))
        .sort();
      const missing = canonicalThemeTokens.filter((token) => !declared.includes(token));
      const extra = declared.filter((token) => !canonicalThemeTokens.includes(token));
      if (missing.length || extra.length) {
        mismatches.push(`${id}: ناقص=[${missing.join(', ')}] زائد=[${extra.join(', ')}]`);
      }
    }
    expect(mismatches).toEqual([]);
  });

  it('وضع «النظام» الداكن يعلن المجموعة الكاملة كذلك', () => {
    const declared = [...collectDeclarations(systemDarkBlock).keys()]
      .filter((token) => !STRUCTURAL_PREFIX.test(token))
      .sort();
    expect(declared).toEqual(canonicalThemeTokens);
  });

  it('الثيمات الملوّنة تختلف فعلاً عن الثيمين الأساسيين — لا ثيم مكرّر', () => {
    const fingerprint = (id: string): string => {
      const block = collectDeclarations(themeBlocks.get(id) ?? '');
      return ['--surface-page', '--surface-card', '--accent', '--text-strong', '--border', '--surface-navy']
        .map((token) => block.get(token)?.value ?? '')
        .join('|');
    };
    const seen = new Map<string, string>();
    for (const id of themeOrder) {
      const key = fingerprint(id);
      expect(key, `الثيم ${id} بلا قيم`).not.toBe('|||||');
      expect(seen.has(key), `الثيم ${id} مكرّر حرفياً لـ${seen.get(key)}`).toBe(false);
      seen.set(key, id);
    }
  });

  it('الرموز البنيوية تُعلن مرة واحدة على :root ولا تتكرر في كتل الثيمات', () => {
    const rootBlock = extractBlock(TOKENS_CSS, ':root {');
    const structuralInRoot = [...collectDeclarations(rootBlock).keys()].filter((token) => STRUCTURAL_PREFIX.test(token));
    expect(structuralInRoot.length).toBeGreaterThan(20);
    // المسافات والطباعة لا تُكرَّر في أي كتلة ثيم.
    const duplicated: string[] = [];
    for (const id of themeOrder) {
      for (const token of collectDeclarations(themeBlocks.get(id) ?? '').keys()) {
        if (/^--(space|radius|fs|lh|ls|weight|font)-/.test(token)) duplicated.push(`${id}:${token}`);
      }
    }
    expect(duplicated).toEqual([]);
  });
});
