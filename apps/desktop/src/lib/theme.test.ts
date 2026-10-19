import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ar } from '../locales/ar';
import { en } from '../locales/en';
import { themeOrder } from './theme-order';
import {
  THEME_STORAGE_KEY,
  applyThemePreference,
  bootstrapTheme,
  isThemePreference,
  readStoredThemePreference,
  resetThemePreference,
  resolveEffectiveTheme,
  setThemePreference,
  themeColorScheme,
  themeHintKey,
  themeLabelKey,
  themePreferences,
} from './theme';

// jsdom لا ينفّذ matchMedia؛ نزرع نسخة قابلة للتحكم لنختبر اتباع ثيم النظام فعلاً.
const installMatchMedia = (matches: boolean): { setMatches: (value: boolean) => void } => {
  const listeners = new Set<(event: { matches: boolean }) => void>();
  let current = matches;
  window.matchMedia = ((query: string) => ({
    matches: current,
    media: query,
    onchange: null,
    addEventListener: (_type: string, listener: (event: { matches: boolean }) => void) => { listeners.add(listener); },
    removeEventListener: (_type: string, listener: (event: { matches: boolean }) => void) => { listeners.delete(listener); },
    addListener: () => undefined,
    removeListener: () => undefined,
    dispatchEvent: () => false,
  })) as unknown as typeof window.matchMedia;
  return {
    setMatches: (value: boolean) => {
      current = value;
      listeners.forEach((listener) => listener({ matches: value }));
    },
  };
};

const appearanceOf = (locale: unknown): Record<string, Record<string, string>> =>
  (locale as { settings: { appearance: Record<string, Record<string, string>> } }).settings.appearance;

describe('إدارة الثيم', () => {
  beforeEach(() => {
    window.localStorage.clear();
    document.documentElement.removeAttribute('data-theme');
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('الافتراضي عند غياب تفضيل مخزّن هو اتباع النظام', () => {
    installMatchMedia(false);
    expect(readStoredThemePreference()).toBe('system');
  });

  it('قيمة مخزّنة غير صالحة تُعامل كـ«النظام» بدل الثقة العمياء', () => {
    window.localStorage.setItem(THEME_STORAGE_KEY, 'neon');
    expect(readStoredThemePreference()).toBe('system');
    expect(isThemePreference('neon')).toBe(false);
    expect(isThemePreference('midnight')).toBe(true);
  });

  it('عرض سبعة تفضيلات: ستة ثيمات صريحة + النظام', () => {
    expect(themePreferences).toHaveLength(7);
    expect(themePreferences).toEqual([...themeOrder, 'system']);
  });

  it('كل ثيم صريح يُكتب في الجذر ويُحفظ ويُطبَّق فوراً', () => {
    installMatchMedia(false);
    for (const theme of themeOrder) {
      expect(setThemePreference(theme)).toBe(themeColorScheme(theme));
      expect(document.documentElement.getAttribute('data-theme')).toBe(theme);
      expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe(theme);
    }
  });

  it('اختيار «النظام» يزيل السمة ليحكم CSS وحده بلا JavaScript', () => {
    installMatchMedia(true);
    applyThemePreference('midnight');
    expect(document.documentElement.hasAttribute('data-theme')).toBe(true);

    expect(setThemePreference('system')).toBe('dark');
    expect(document.documentElement.hasAttribute('data-theme')).toBe(false);
  });

  it('الثيم المستمر يُطبَّق عند الإقلاع قبل أول رسم', () => {
    installMatchMedia(false);
    window.localStorage.setItem(THEME_STORAGE_KEY, 'emerald');
    expect(bootstrapTheme()).toBe('emerald');
    expect(document.documentElement.getAttribute('data-theme')).toBe('emerald');
  });

  it('استعادة المظهر الافتراضي تعيد الاتباع للنظام', () => {
    installMatchMedia(true);
    setThemePreference('sand');
    expect(resetThemePreference()).toBe('dark');
    expect(document.documentElement.hasAttribute('data-theme')).toBe(false);
    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe('system');
  });

  it('عائلة السطوع صحيحة لكل ثيم ولا لبس فيها', () => {
    installMatchMedia(true);
    expect(resolveEffectiveTheme('system')).toBe('dark');
    installMatchMedia(false);
    expect(resolveEffectiveTheme('system')).toBe('light');
    // الثيمات الفاتحة لا تتأثر بسمة النظام، والعكس.
    installMatchMedia(true);
    expect(resolveEffectiveTheme('light')).toBe('light');
    expect(resolveEffectiveTheme('sand')).toBe('light');
    expect(resolveEffectiveTheme('midnight')).toBe('dark');
    expect(resolveEffectiveTheme('emerald')).toBe('dark');
    expect(resolveEffectiveTheme('contrast')).toBe('dark');
  });

  // سكربت الإقلاع المبكر يعمل قبل تحميل أي وحدة، فهو نسخة ثانية من منطق بسيط.
  // هذا الاختبار يمنع انحراف مفتاح التخزين أو قائمة الثيمات بين النسختين بصمت.
  it('سكربت الإقلاع المبكر متوافق مع مفتاح التخزين وقائمة الثيمات المولَّدة', () => {
    const script = readFileSync(resolve(process.cwd(), 'public', 'theme-boot.js'), 'utf8');
    expect(script).toContain(`'${THEME_STORAGE_KEY}'`);
    for (const theme of themeOrder) {
      expect(script, `الثيم ${theme} غائب عن سكربت الإقلاع`).toContain(`'${theme}'`);
    }
    expect(script).toContain('data-theme');
    // لا يجوز أن يكتب السكربت قيمة لتفضيل «النظام»: الغياب هو الدلالة.
    expect(script).not.toContain("setAttribute('data-theme', 'system')");
    expect(script).toContain('catch');
  });

  // الثيمة بلا ترجمة تظهر مفتاحاً خاماً للمستخدم — وهذا يمنعه في اللغتين.
  it('كل ثيم له اسم ووصف مترجمان في العربية والإنجليزية', () => {
    for (const theme of themeOrder) {
      expect(themeLabelKey(theme)).toBe(`settings.appearance.theme.${theme}`);
      expect(themeHintKey(theme)).toBe(`settings.appearance.themeHint.${theme}`);
      for (const [language, locale] of [['ar', ar], ['en', en]] as const) {
        const appearance = appearanceOf(locale);
        expect(appearance.theme?.[theme], `${language}: اسم الثيم ${theme}`).toBeTruthy();
        expect(appearance.themeHint?.[theme], `${language}: وصف الثيم ${theme}`).toBeTruthy();
      }
    }
    // ومفاتيح «النظام» كذلك.
    for (const [language, locale] of [['ar', ar], ['en', en]] as const) {
      const appearance = appearanceOf(locale);
      expect(appearance.system, `${language}: اسم النظام`).toBeTruthy();
      expect(appearance.systemHint, `${language}: وصف النظام`).toBeTruthy();
      expect(appearance.scheme?.light, `${language}: مخطط فاتح`).toBeTruthy();
      expect(appearance.scheme?.dark, `${language}: مخطط داكن`).toBeTruthy();
    }
  });
});
