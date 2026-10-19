import i18n from 'i18next';
import LanguageDetector from 'i18next-browser-languagedetector';
import { initReactI18next } from 'react-i18next';
import { ar } from '../locales/ar';
import { en } from '../locales/en';

export const supportedLanguages = ['ar', 'en'] as const;
export type SupportedLanguage = (typeof supportedLanguages)[number];

export const isSupportedLanguage = (value: string): value is SupportedLanguage =>
  supportedLanguages.includes(value as SupportedLanguage);

export const applyDocumentLanguage = (language: string): void => {
  const normalizedLanguage = isSupportedLanguage(language) ? language : 'ar';
  document.documentElement.lang = normalizedLanguage;
  document.documentElement.dir = normalizedLanguage === 'ar' ? 'rtl' : 'ltr';
};

void i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources: {
      ar: { translation: ar },
      en: { translation: en },
    },
    fallbackLng: 'ar',
    supportedLngs: supportedLanguages,
    interpolation: {
      escapeValue: false,
    },
    detection: {
      order: ['localStorage', 'navigator'],
      caches: ['localStorage'],
    },
  });

applyDocumentLanguage(i18n.resolvedLanguage ?? i18n.language);
i18n.on('languageChanged', applyDocumentLanguage);

export { i18n };
