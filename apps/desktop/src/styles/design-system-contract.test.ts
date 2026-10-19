import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

// عقد نظام التصميم.
//
// سبب وجوده: عيب مؤكَّد (CQ-05) — 25 صنفاً مُستشهداً في الواجهة بلا أي تعريف في
// ملفات الأنماط، فظهرت الوحدات بلا إيقاع موحّد وبلا حاويات صحيحة، وتجمّعت ديون
// بصرية بلا أي حارس يكشفها. وكذلك عيب (CQ-01): رموز لونية دائرية أسقطت كل الحدود.
//
// النطاق المقصود: الأصناف الساكنة فقط (`className="a b"` و`className={'a b'}`).
// الأصناف المبنية ديناميكياً من قوالب نصية لا تُفحص هنا لأن محتواها تعبيرات لا أصناف.

const SRC_ROOT = resolve(process.cwd(), 'src');
const CLASS_TOKEN = /^-?[_a-zA-Z][\w-]*$/;

const walk = (directory: string): string[] => {
  const out: string[] = [];
  for (const entry of readdirSync(directory)) {
    const full = join(directory, entry);
    if (statSync(full).isDirectory()) out.push(...walk(full));
    else out.push(full);
  }
  return out;
};

const allFiles = walk(SRC_ROOT);
const stylesheets = allFiles.filter((file) => file.endsWith('.css'));
const components = allFiles.filter((file) => file.endsWith('.tsx') && !file.endsWith('.test.tsx'));

const cssSource = stylesheets
  .map((file) => readFileSync(file, 'utf8'))
  .join('\n')
  .replace(/\/\*[\s\S]*?\*\//g, '');

const definedClasses = new Set<string>();
for (const match of cssSource.matchAll(/\.(-?[_a-zA-Z][\w-]*)/g)) definedClasses.add(match[1]);

const usedClasses = new Map<string, Set<string>>();
const record = (token: string, file: string): void => {
  if (!CLASS_TOKEN.test(token)) return;
  if (!usedClasses.has(token)) usedClasses.set(token, new Set());
  usedClasses.get(token)!.add(file);
};

for (const file of components) {
  const relativePath = relative(SRC_ROOT, file).replace(/\\/g, '/');
  const source = readFileSync(file, 'utf8');
  for (const match of source.matchAll(/className="([^"]*)"/g)) {
    for (const token of match[1].split(/\s+/)) record(token, relativePath);
  }
  for (const match of source.matchAll(/className=\{'([^']*)'\}/g)) {
    for (const token of match[1].split(/\s+/)) record(token, relativePath);
  }
}

describe('عقد نظام التصميم', () => {
  it('كل صنف CSS ساكن مُستشهد في الواجهة له تعريف فعلي في ملفات الأنماط', () => {
    const orphans = [...usedClasses.entries()]
      .filter(([name]) => !definedClasses.has(name))
      .map(([name, where]) => `${name} <- ${[...where].sort().join(', ')}`)
      .sort();
    expect(orphans).toEqual([]);
  });

  it('لا توجد ألوان مكتوبة يدوياً داخل مكوّنات الواجهة', () => {
    const offenders: string[] = [];
    for (const file of components) {
      const relativePath = relative(SRC_ROOT, file).replace(/\\/g, '/');
      const source = readFileSync(file, 'utf8');
      // اللون يجب أن يأتي من رموز التصميم لا من قيمة حرفية في الشيفرة.
      for (const pattern of [/#[0-9a-fA-F]{3,8}\b/g, /\brgba?\(/g, /\bhsla?\(/g]) {
        for (const match of source.matchAll(pattern)) offenders.push(`${relativePath}: ${match[0]}`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it('لا توجد قيم font-family حرفية في ملفات الأنماط خارج طبقة الرموز وتعريفات الخط', () => {
    const offenders: string[] = [];
    for (const file of stylesheets) {
      const relativePath = relative(SRC_ROOT, file).replace(/\\/g, '/');
      // tokens.css يعلن كومة العائلات، وfonts.css يعرّف الوجوه المحلية بـ@font-face —
      // كلاهما طبقة تعريف مشروعة؛ القيمة الحرفية في مكوّن/تخطيط هي المخالفة.
      if (relativePath.endsWith('tokens.css') || relativePath.endsWith('fonts.css')) continue;
      const source = readFileSync(file, 'utf8').replace(/\/\*[\s\S]*?\*\//g, '');
      for (const match of source.matchAll(/font-family\s*:\s*([^;]+);/g)) {
        const value = match[1].trim();
        // الاستهلاك من رموز الطباعة هو المطلوب؛ القيمة الحرفية هي المخالفة.
        if (value.startsWith('var(') || value.startsWith('inherit')) continue;
        offenders.push(`${relativePath}: ${value}`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it('لا توجد أحجام خطوط حرفية في ملفات الأنماط خارج طبقة الرموز', () => {
    const offenders: string[] = [];
    for (const file of stylesheets) {
      const relativePath = relative(SRC_ROOT, file).replace(/\\/g, '/');
      if (relativePath.endsWith('tokens.css')) continue;
      const source = readFileSync(file, 'utf8').replace(/\/\*[\s\S]*?\*\//g, '');
      for (const match of source.matchAll(/font-size\s*:\s*([^;]+);/g)) {
        const value = match[1].trim();
        if (value.startsWith('var(') || value.startsWith('inherit')) continue;
        offenders.push(`${relativePath}: ${value}`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it('لا توجد قيم لونية حرفية في ملفات الأنماط خارج طبقة الرموز', () => {
    const offenders: string[] = [];
    for (const file of stylesheets) {
      const relativePath = relative(SRC_ROOT, file).replace(/\\/g, '/');
      if (relativePath.endsWith('tokens.css')) continue;
      const source = readFileSync(file, 'utf8').replace(/\/\*[\s\S]*?\*\//g, '');
      for (const match of source.matchAll(/#[0-9a-fA-F]{3,8}\b/g)) offenders.push(`${relativePath}: ${match[0]}`);
    }
    expect(offenders).toEqual([]);
  });

  // ===== نظام الأيقونات =====
  // عيب مؤكَّد أُصلح (CQ-08): كانت الواجهة تستعمل تسعة مقاسات أيقونات مختلفة
  // (14/15/16/17/18/20/22) بلا قاعدة، فتظهر الأيقونات متفاوتة الحجم في الصفحات
  // المتشابهة. المقاسات المسموحة الآن أربعة فقط، ولكل واحد دلالة معلنة.

  it('كل مقاسات الأيقونات من السلّم المعتمد 14/16/18/20 فقط', () => {
    const allowed = new Set([14, 16, 18, 20]);
    const offenders: string[] = [];
    for (const file of components) {
      const relativePath = relative(SRC_ROOT, file).replace(/\\/g, '/');
      const source = readFileSync(file, 'utf8');
      for (const match of source.matchAll(/size=\{(\d+)\}/g)) {
        const size = Number(match[1]);
        if (!allowed.has(size)) offenders.push(`${relativePath}: size={${size}}`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it('لا تُفرض سماكة خط مخصّصة على أي أيقونة حتى لا تتفاوت الأيقونات', () => {
    const offenders: string[] = [];
    for (const file of components) {
      const relativePath = relative(SRC_ROOT, file).replace(/\\/g, '/');
      const source = readFileSync(file, 'utf8');
      if (/strokeWidth/.test(source)) offenders.push(relativePath);
    }
    expect(offenders).toEqual([]);
  });

  it('لا توجد أيقونات SVG مضمّنة يدوياً ولا رموز تعبيرية في واجهة الإنتاج', () => {
    const offenders: string[] = [];
    // نطاقات الرموز التعبيرية والتصويرية — تُفحص منفصلة تجنّباً لخلط محارف مركّبة.
    const emojiPatterns = [/[\u{1F300}-\u{1FAFF}]/u, /[\u{2600}-\u{27BF}]/u, /[\u{FE0F}]/u];
    const hasEmoji = (text: string): boolean => emojiPatterns.some((pattern) => pattern.test(text));
    for (const file of components) {
      const relativePath = relative(SRC_ROOT, file).replace(/\\/g, '/');
      const source = readFileSync(file, 'utf8');
      if (/<svg[\s>]/.test(source)) offenders.push(`${relativePath}: inline <svg>`);
      if (hasEmoji(source)) offenders.push(`${relativePath}: emoji`);
    }
    // وكذلك ملفات الترجمة: لا رموز تعبيرية في نصوص المنتج.
    for (const locale of ['../locales/ar.ts', '../locales/en.ts']) {
      const source = readFileSync(new URL(locale, import.meta.url), 'utf8');
      if (hasEmoji(source)) offenders.push(`${locale}: emoji`);
    }
    expect(offenders).toEqual([]);
  });

  it('مكتبة الأيقونات واحدة: lucide-react فقط', () => {
    const offenders: string[] = [];
    for (const file of components) {
      const relativePath = relative(SRC_ROOT, file).replace(/\\/g, '/');
      const source = readFileSync(file, 'utf8');
      for (const match of source.matchAll(/from ['"]([^'"]*icon[^'"]*|@?[a-z-]*icons?[^'"]*)['"]/gi)) {
        if (!match[1].includes('lucide-react')) offenders.push(`${relativePath}: ${match[1]}`);
      }
    }
    expect(offenders).toEqual([]);
  });
});
