// إدارة الثيم: تفضيل المستخدم (ستة ثيمات + «النظام») مع حفظ دائم ومنع وميض الإقلاع.
//
// التصميم: عندما يكون التفضيل «النظام» لا نكتب `data-theme` إطلاقاً، فيتولّى
// `@media (prefers-color-scheme: dark)` في `tokens.css` اختيار اللوحة بلا JavaScript.
// أما الثيمات الصريحة فتُكتب كسمة، وكل ثيم يعلن لوحته الدلالية الكاملة في `tokens.css`
// فلا يرث قيمة ثيم آخر.

import { themeOrder } from './theme-order';

/** معرّفات الثيمات الصريحة. الترتيب مصدره `theme-order.ts` المشترك مع المولّد. */
export const themeIds = themeOrder;
export type ThemeId = (typeof themeIds)[number];

/** التفضيل يشمل «النظام» فوق الثيمات الصريحة. */
export const themePreferences = [...themeIds, 'system'] as const;
export type ThemePreference = (typeof themePreferences)[number];

/** الوضع المحسوم فعلاً — «النظام» يُحسم إلى فاتح أو داكن حسب سمة ويندوز. */
export type EffectiveTheme = 'light' | 'dark';

/** مفتاح التخزين المحلي لتفضيل الثيم — ثابت واحد يمنع انحراف المفاتيح. */
export const THEME_STORAGE_KEY = 'nexus.theme';

/** الاستعلام الذي يخبرنا بلوحة النظام الفعلية (للمعاينة والتسمية فقط). */
const darkSchemeQuery = '(prefers-color-scheme: dark)';

export const isThemeId = (value: unknown): value is ThemeId =>
  typeof value === 'string' && (themeIds as readonly string[]).includes(value);

export const isThemePreference = (value: unknown): value is ThemePreference =>
  typeof value === 'string' && (themePreferences as readonly string[]).includes(value);

/** قراءة التفضيل المخزّن بأمان: أي قيمة غير صالحة تُعامل كـ«النظام». */
export const readStoredThemePreference = (): ThemePreference => {
  try {
    const stored = window.localStorage.getItem(THEME_STORAGE_KEY);
    return isThemePreference(stored) ? stored : 'system';
  } catch {
    // التخزين المحلي قد يكون محجوباً في سياق مقيد — الافتراضي الآمن هو اتباع النظام.
    return 'system';
  }
};

/** هل نظام التشغيل حالياً داكن؟ */
export const systemPrefersDark = (): boolean => {
  if (typeof window.matchMedia !== 'function') return false;
  return window.matchMedia(darkSchemeQuery).matches;
};

/**
 * اللوحة الفعلية المطبَّقة الآن.
 * - «النظام» ⇒ فاتح/داكن حسب ويندوز.
 * - ثيم صريح: يعيد نفسه إن كان فاتحاً أو داكناً. أما الثيمات الملوّنة (منتصف الليل ·
 *   رملي · زمردي · تباين) فتُعاد حسب عائلة سطوعها لأن الواجهة لا تحتاج أكثر من ذلك
 *   لوصف «المطبَّق الآن»؛ والتسمية الدقيقة تأتي من `themeLabelKey`.
 */
export const resolveEffectiveTheme = (preference: ThemePreference): EffectiveTheme => {
  if (preference === 'system') return systemPrefersDark() ? 'dark' : 'light';
  return themeColorScheme(preference);
};

/** عائلة السطوع لكل ثيم — تُستخدم لضبط `color-scheme` في الواجهة وللتسمية. */
export const themeColorScheme = (theme: ThemeId): EffectiveTheme =>
  theme === 'light' || theme === 'sand' ? 'light' : 'dark';

/**
 * تطبيق التفضيل على جذر المستند.
 * - «النظام»: إزالة السمة ليحكم استعلام الوسائط.
 * - ثيم صريح: كتابة معرّفه في `data-theme`.
 * - الوضع الفاتح: تُزال السمة أيضاً عندما يكون الثيم الفاتح هو الافتراضي، فلا فرق
 *   بصرياً بين «فاتح» و«بلا سمة» في بيئة فاتحة. لكن نكتبها صراحةً ليكون الاختيار
 *   قاطعاً حتى لو كان النظام داكناً.
 */
export const applyThemePreference = (preference: ThemePreference): EffectiveTheme => {
  const root = document.documentElement;
  if (preference === 'system') {
    root.removeAttribute('data-theme');
  } else {
    root.setAttribute('data-theme', preference);
  }
  return resolveEffectiveTheme(preference);
};

/** حفظ التفضيل ثم تطبيقه — الدالة الوحيدة التي تكتب التخزين. */
export const setThemePreference = (preference: ThemePreference): EffectiveTheme => {
  try {
    window.localStorage.setItem(THEME_STORAGE_KEY, preference);
  } catch {
    // فشل الحفظ لا يجب أن يمنع التطبيق الفوري للتفضيل في الجلسة الحالية.
  }
  return applyThemePreference(preference);
};

/** استعادة المظهر الافتراضي (اتباع النظام). */
export const resetThemePreference = (): EffectiveTheme => setThemePreference('system');

/**
 * تطبيق التفضيل المخزّن مبكراً — تُستدعى قبل `createRoot` لمنع وميض الثيم.
 */
export const bootstrapTheme = (): ThemePreference => {
  const preference = readStoredThemePreference();
  applyThemePreference(preference);
  return preference;
};

/**
 * الاشتراك في تغيّر لوحة النظام. يُستخدم لتحديث المعاينة والتسمية عندما يكون
 * التفضيل «النظام»، فلا تبقى الواجهة تعرض «فاتح» بينما النظام انتقل إلى الداكن.
 */
export const subscribeToSystemTheme = (listener: (theme: EffectiveTheme) => void): (() => void) => {
  if (typeof window.matchMedia !== 'function') return () => undefined;
  const mediaQuery = window.matchMedia(darkSchemeQuery);
  const handler = (): void => listener(mediaQuery.matches ? 'dark' : 'light');
  mediaQuery.addEventListener('change', handler);
  return () => mediaQuery.removeEventListener('change', handler);
};

/** مفتاح الترجمة لاسم أي تفضيل (الثيمات الصريحة و«النظام»). */
export const themeLabelKey = (preference: ThemePreference): string =>
  preference === 'system' ? 'settings.appearance.system' : `settings.appearance.theme.${preference}`;

/** مفتاح الترجمة لوصف أي تفضيل. */
export const themeHintKey = (preference: ThemePreference): string =>
  preference === 'system' ? 'settings.appearance.systemHint' : `settings.appearance.themeHint.${preference}`;
