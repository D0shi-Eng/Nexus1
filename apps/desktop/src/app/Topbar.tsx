import { Globe2, Menu, Search } from 'lucide-react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { nexusApi } from '../lib/nexus-api';

// اختصار لوحة الأوامر يختلف بين المنصات؛ عرض ⌘ داخل WebView2 على ويندوز يضلل المستخدم
// عن المفتاح الذي يعمل فعلاً، ولهذا يُشتق النص من منصة التشغيل لا من افتراض ثابت.
const isApplePlatform = (): boolean => /mac|iphone|ipad/i.test(navigator.userAgent);

// الشريط العلوي للقشرة كلها — البحث واللغة ومؤشر الصحة متاحان في كل وحدة لا في اللوحة فقط.
export const Topbar = ({ onOpenSearch, onOpenSidebar }: { onOpenSearch: () => void; onOpenSidebar: () => void }) => {
  const { t, i18n } = useTranslation();
  const healthQuery = useQuery({
    queryKey: ['system-health'],
    queryFn: nexusApi.getHealth,
    staleTime: 30_000,
  });
  const language = i18n.resolvedLanguage ?? 'ar';
  const shortcutHint = isApplePlatform() ? '⌘ K' : 'Ctrl K';
  // غياب النتيجة ليس «غير متاح»: عرض حالة سلبية قبل وصول الفحص ادعاء غير صحيح.
  const healthLabel = healthQuery.isLoading
    ? t('system.checking')
    : healthQuery.data?.application === 'HEALTHY' ? t('system.healthy') : t('system.unavailable');

  const switchLanguage = async (): Promise<void> => {
    await i18n.changeLanguage(language === 'ar' ? 'en' : 'ar');
  };

  return (
    <header className="topbar">
      <button className="icon-button mobile-menu" type="button" onClick={onOpenSidebar} aria-label={t('accessibility.mainNavigation')}>
        <Menu size={20} />
      </button>
      <button className="topbar__search" type="button" onClick={onOpenSearch} aria-label={t('accessibility.commandPalette')}>
        <Search size={18} aria-hidden="true" />
        <span>{t('actions.command')}</span>
        <kbd>{shortcutHint}</kbd>
      </button>
      <div className="topbar__actions">
        {/* زر تبديل لا قائمة منسدلة — إزالة سهم القائمة تمنع انتظار قائمة لا تُفتح أبداً. */}
        <button
          className="language-button"
          type="button"
          onClick={() => { void switchLanguage(); }}
          aria-label={t('actions.changeLanguage')}
          title={t('actions.changeLanguage')}
        >
          <Globe2 size={16} aria-hidden="true" />
          <span>{language === 'ar' ? 'EN' : 'AR'}</span>
        </button>
        <div className="health-indicator" role="status" aria-label={t('accessibility.systemHealth')}>
          <span className={`health-dot health-dot--${healthQuery.data?.application === 'HEALTHY' ? 'healthy' : 'unknown'}`} aria-hidden="true" />
          <span>{healthLabel}</span>
        </div>
      </div>
    </header>
  );
};
