import { useEffect, useState } from 'react';
import { Check, Moon, RotateCcw, Sun, SunMoon } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import {
  readStoredThemePreference,
  resetThemePreference,
  resolveEffectiveTheme,
  setThemePreference,
  subscribeToSystemTheme,
  themeColorScheme,
  themeHintKey,
  themePreferences,
  type EffectiveTheme,
  type ThemePreference,
} from '../../lib/theme';

// لوحة المظهر: ستة ثيمات حقيقية + اتباع النظام.
// المعاينة حقيقية لأن كل بطاقة تحمل `data-theme` الخاص بها، فيُعيد النطاق الفرعي
// لوحة ذلك الثيم فعلياً من `tokens.css` — لا نسخة ألوان ثانية يمكن أن تنحرف.
const preferenceIcons = {
  light: Sun,
  dark: Moon,
  system: SunMoon,
} as const;

export const AppearancePanel = () => {
  const { t } = useTranslation();
  const [preference, setPreference] = useState<ThemePreference>(() => readStoredThemePreference());
  const [effective, setEffective] = useState<EffectiveTheme>(() => resolveEffectiveTheme(readStoredThemePreference()));

  // عند اتباع النظام نبقى متزامنين مع تغيّر سمة ويندوز أثناء فتح الصفحة.
  useEffect(() => {
    if (preference !== 'system') return undefined;
    return subscribeToSystemTheme((theme) => setEffective(theme));
  }, [preference]);

  const choose = (next: ThemePreference): void => {
    setPreference(next);
    setEffective(setThemePreference(next));
  };

  const reset = (): void => {
    setPreference('system');
    setEffective(resetThemePreference());
  };

  // «المطبَّق الآن»: الثيم الصريح باسمه، و«النظام» باللوحة التي حسمها ويندوز.
  const effectiveLabel = preference === 'system'
    ? `${t('settings.appearance.system')} · ${t(`settings.appearance.theme.${effective}`)}`
    : t(`settings.appearance.theme.${preference}`);

  return (
    <section className="panel" aria-labelledby="settings-appearance-title">
      <div className="panel__header">
        <div><h2 id="settings-appearance-title">{t('settings.appearance.title')}</h2></div>
        <button className="button button--secondary" type="button" onClick={reset}>
          <RotateCcw size={16} aria-hidden="true" />
          {t('settings.appearance.reset')}
        </button>
      </div>

      <fieldset className="theme-options">
        <legend className="visually-hidden">{t('settings.appearance.themeLabel')}</legend>
        {themePreferences.map((option) => {
          const SystemIcon = preferenceIcons.system;
          const isActive = preference === option;
          // نطاق المعاينة: «النظام» يعرض اللوحة التي حسمها النظام الآن.
          const previewScope = option === 'system' ? effective : option;
          return (
            <label
              key={option}
              className={`theme-option ${isActive ? 'theme-option--active' : ''}`}
              data-theme={previewScope}
            >
              <span className="theme-option__label">
                <input
                  type="radio"
                  name="nexus-theme"
                  value={option}
                  checked={isActive}
                  onChange={() => choose(option)}
                />
                {option === 'system' ? <SystemIcon size={16} aria-hidden="true" /> : null}
                {option === 'system' ? t('settings.appearance.system') : t(`settings.appearance.theme.${option}`)}
                {isActive ? <Check className="theme-option__check" size={14} aria-hidden="true" /> : null}
              </span>
              <span className="theme-option__swatches" aria-hidden="true">
                <span className="theme-option__swatch theme-option__swatch--page" />
                <span className="theme-option__swatch theme-option__swatch--card" />
                <span className="theme-option__swatch theme-option__swatch--accent" />
                <span className="theme-option__swatch theme-option__swatch--gold" />
              </span>
              <small>{t(themeHintKey(option))}</small>
              {option === 'system' ? null : (
                <small className="theme-option__scheme">
                  {t(`settings.appearance.scheme.${themeColorScheme(option)}`)}
                </small>
              )}
            </label>
          );
        })}
      </fieldset>

      <div className="settings-field">
        <small className="form-note">
          {t('settings.appearance.effectiveNow', { theme: effectiveLabel })}
        </small>
      </div>

      <p className="form-note settings-note">{t('settings.appearance.note')}</p>
    </section>
  );
};
