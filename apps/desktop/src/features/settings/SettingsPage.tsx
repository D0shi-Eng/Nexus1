import { useMemo, useState } from 'react';
import {
  Building2,
  CircleAlert,
  Cpu,
  Database,
  Info,
  KeyRound,
  Languages,
  LockKeyhole,
  Palette,
  PlugZap,
  Settings2,
  SlidersHorizontal,
} from 'lucide-react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { i18n, isSupportedLanguage, supportedLanguages } from '../../lib/i18n';
import { nexusApi } from '../../lib/nexus-api';
import { useSession } from '../../lib/use-session';
import { AppearancePanel } from './AppearancePanel';
import { BackupPanel } from './BackupPanel';
import { ChangePasswordPanel } from './ChangePasswordPanel';
import { ModelsPanel } from './ModelsPanel';
import { ProvidersPanel } from './ProvidersPanel';
import { UsersPanel } from './UsersPanel';

// الوحدة 15 — الإعدادات.
// المبدأ: تُعرض الأقسام المنفذة فعلاً فقط، وكل ما ليس منفذاً يُذكر بصدق داخل
// قسم «حول والتشخيص» بدل عرضه كإعداد زائف. لا تُدار سياسات أمنية ولا صلاحيات من هنا؛
// الخلف هو الحكم، والواجهة تعرض وتتحقق فقط.
// 0026: أُضيف قسم «الأمان» لتغيير كلمة المرور — للمستخدم نفسه في وضع الإنتاج فقط.
type SettingsSectionKey = 'general' | 'appearance' | 'language' | 'providers' | 'models' | 'data' | 'security' | 'about';

const sectionIcons = {
  general: Settings2,
  appearance: Palette,
  language: Languages,
  providers: PlugZap,
  models: Cpu,
  data: Database,
  security: KeyRound,
  about: Info,
} as const;

export const SettingsPage = () => {
  const { t } = useTranslation();
  const { displayName, responsibility, can } = useSession();
  const [activeSection, setActiveSection] = useState<SettingsSectionKey>('general');
  const language = i18n.resolvedLanguage ?? 'ar';

  // وضع التشغيل يأتي من بوابة الهوية المخزّنة مؤقتاً؛ وسم الجلسة ومستأجرها يتبعانه
  // حتى لا تظهر مساحة إنتاج موسومة كمساحة تجريبية.
  const authStatusQuery = useQuery({
    queryKey: ['auth-status'],
    queryFn: nexusApi.getAuthStatus,
    staleTime: 5_000,
  });
  const healthQuery = useQuery({
    queryKey: ['system-health'],
    queryFn: nexusApi.getHealth,
    staleTime: 30_000,
  });
  const isProduction = authStatusQuery.data?.mode === 'PRODUCTION';

  const changeLanguage = (nextLanguage: string): void => {
    if (isSupportedLanguage(nextLanguage)) {
      void i18n.changeLanguage(nextLanguage);
    }
  };

  // الأقسام المدرجة مشتقة من الصلاحيات الفعلية: قسم البيانات لا يظهر لمن لا يملك
  // أي صلاحية داخله، وقسما الموفرين والنماذج يظهران لمن يملك قراءة التكاملات فقط،
  // فلا يبقى قسم فارغ بلا معنى ولا قدرة وهمية.
  const sections = useMemo<SettingsSectionKey[]>(() => {
    const keys: SettingsSectionKey[] = ['general', 'appearance', 'language'];
    if (can('integration:read')) keys.push('providers', 'models');
    if (can('user:read') || can('backup:create')) keys.push('data');
    // كلمة المرور تخصّ المستخدم نفسه، ولا وجود لمسارها في وضع الديمو (جلسات تركيبية).
    if (isProduction) keys.push('security');
    keys.push('about');
    return keys;
  }, [can, isProduction]);

  return (
    <section id="settings" className="content-container" aria-labelledby="settings-title" aria-live="polite">
      <header className="page-heading">
        <div>
          <span className="eyebrow"><SlidersHorizontal size={14} aria-hidden="true" /> {t('settings.eyebrow')}</span>
          <h1 id="settings-title">{t('settings.title')}</h1>
          <p>{t('settings.subtitle')}</p>
        </div>
      </header>

      <div className="settings-layout">
        <nav className="settings-nav" aria-label={t('settings.sections.label')}>
          {sections.map((key) => {
            const Icon = sectionIcons[key];
            const isActive = activeSection === key;
            return (
              <button
                key={key}
                type="button"
                className={`settings-nav__item ${isActive ? 'settings-nav__item--active' : ''}`}
                aria-current={isActive ? 'true' : undefined}
                onClick={() => setActiveSection(key)}
              >
                <Icon size={16} aria-hidden="true" />
                <span>{t(`settings.sections.${key}`)}</span>
              </button>
            );
          })}
        </nav>

        <div className="settings-panel">
          {activeSection === 'general' ? (
            <section className="panel" aria-labelledby="settings-general-title">
              <div className="panel__header">
                <div><h2 id="settings-general-title">{t('settings.general.title')}</h2></div>
                <Building2 size={18} aria-hidden="true" />
              </div>
              <dl className="details-list">
                <div><dt>{t('settings.general.productName')}</dt><dd>{t('product.name')}</dd></div>
                <div><dt>{t('settings.general.productSubtitle')}</dt><dd>{t('product.subtitle')}</dd></div>
                <div>
                  <dt>{t('settings.general.modeLabel')}</dt>
                  <dd>{isProduction ? t('settings.general.modeProduction') : t('settings.general.modeDemo')}</dd>
                </div>
              </dl>
              <p className="form-note settings-note">{t('settings.general.note')}</p>
            </section>
          ) : null}

          {activeSection === 'appearance' ? <AppearancePanel /> : null}

          {activeSection === 'providers' ? <ProvidersPanel /> : null}
          {activeSection === 'models' ? <ModelsPanel /> : null}

          {activeSection === 'language' ? (
            <section className="panel" aria-labelledby="settings-language-title">
              <div className="panel__header">
                <div><h2 id="settings-language-title">{t('settings.language.title')}</h2></div>
                <Languages size={18} aria-hidden="true" />
              </div>
              <label className="settings-field">
                <span>{t('settings.language.label')}</span>
                <select value={language} onChange={(event) => changeLanguage(event.target.value)}>
                  {supportedLanguages.map((code) => (
                    <option key={code} value={code}>{t(`settings.language.${code}`)}</option>
                  ))}
                </select>
              </label>
              <p className="form-note settings-note">{t('settings.language.note')}</p>
            </section>
          ) : null}

          {activeSection === 'data' ? (
            <>
              <section className="panel" aria-labelledby="settings-session-title">
                <div className="panel__header">
                  <div><h2 id="settings-session-title">{isProduction ? t('settings.session.titleProduction') : t('settings.session.title')}</h2></div>
                  <Building2 size={18} aria-hidden="true" />
                </div>
                <dl className="details-list">
                  <div><dt>{t('settings.session.actor')}</dt><dd>{displayName || '—'}</dd></div>
                  <div><dt>{t('settings.session.responsibility')}</dt><dd>{t(`identity.responsibility.${responsibility || 'REQUESTER'}`, { defaultValue: responsibility })}</dd></div>
                  <div><dt>{t('settings.session.tenant')}</dt><dd>{isProduction ? t('system.productionTenant') : t('system.demoTenant')}</dd></div>
                </dl>
                <p className="form-note settings-note">{isProduction ? t('settings.session.noteProduction') : t('settings.session.note')}</p>
              </section>
              {can('user:read') ? <UsersPanel /> : null}
              {can('backup:create') ? <BackupPanel /> : null}
            </>
          ) : null}

          {activeSection === 'security' ? <ChangePasswordPanel /> : null}

          {activeSection === 'about' ? (
            <>
              <section className="panel" aria-labelledby="settings-about-title">
                <div className="panel__header">
                  <div><h2 id="settings-about-title">{t('settings.about.title')}</h2></div>
                  <Info size={18} aria-hidden="true" />
                </div>
                <dl className="details-list">
                  <div><dt>{t('settings.general.productName')}</dt><dd>{t('product.name')}</dd></div>
                  <div><dt>{t('settings.general.modeLabel')}</dt><dd>{isProduction ? t('settings.general.modeProduction') : t('settings.general.modeDemo')}</dd></div>
                  <div>
                    <dt>{t('settings.about.healthTitle')}</dt>
                    <dd>
                      {healthQuery.data
                        ? `${healthQuery.data.application} · ${healthQuery.data.database} · ${healthQuery.data.schemaVersion ?? '—'}`
                        : t('system.checking')}
                    </dd>
                  </div>
                </dl>
                <p className="form-note settings-note">{t('settings.about.note')}</p>
              </section>

              <section className="panel" aria-labelledby="settings-unavailable-title">
                <div className="panel__header">
                  <div><h2 id="settings-unavailable-title">{t('settings.unavailable.title')}</h2></div>
                  <LockKeyhole size={18} aria-hidden="true" />
                </div>
                <ul className="settings-unavailable">
                  <li><CircleAlert size={16} aria-hidden="true" /><span>{t('settings.unavailable.securityPolicy')}</span></li>
                  <li><CircleAlert size={16} aria-hidden="true" /><span>{t('settings.unavailable.providerConfig')}</span></li>
                </ul>
                <p className="form-note settings-note">{t('settings.unavailable.note')}</p>
              </section>
            </>
          ) : null}
        </div>
      </div>
    </section>
  );
};
