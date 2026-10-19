import { Cpu } from 'lucide-react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { nexusApi } from '../lib/nexus-api';
import { useSession } from '../lib/use-session';

// محدّد النموذج المشترك لكل مسارات الذكاء الاصطناعي.
//
// الغاية: مسار واحد لاختيار النموذج بدل نسخة في كل سطح. الاختيار مخزَّن خلفياً
// بحالة واحدة مختارة، فيقرأه تنفيذ الوكيل والاسترجاع والمخطِّط والمدقِّق من المصدر
// نفسه ولا يمكن أن تتباعد بينها. لا fallback صامت إلى نموذج آخر: إن لم يوجد اختيار
// صالح يُعلن ذلك صراحةً ويبقى التنفيذ مغلقاً.
//
// لا يعرض هذا المكوّن أي قدرة غير معلنة: القدرات المجهولة تظهر «غير معروف».
export const ModelSelector = ({ id = 'model-selector' }: { id?: string }) => {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { can } = useSession();
  const canRead = can('integration:read');
  const canSelect = can('integration:update');

  const modelsQuery = useQuery({
    queryKey: ['ai-models'],
    queryFn: nexusApi.getAiModels,
    staleTime: 10_000,
    enabled: canRead,
  });

  const selectMutation = useMutation({
    mutationFn: nexusApi.selectAiModel,
    onSuccess: (data) => {
      queryClient.setQueryData(['ai-models'], data);
      // تغيير النموذج يبطل أي حالة اشتقاقية تعتمد عليه في بقية الأسطح.
      void queryClient.invalidateQueries({ queryKey: ['ai-providers'] });
      void queryClient.invalidateQueries({ queryKey: ['integration-status'] });
    },
  });

  if (!canRead) return null;

  const models = modelsQuery.data?.models ?? [];
  const available = models.filter((model) => model.available);
  const selected = models.find((model) => model.selected) ?? null;
  const selectedValue = selected ? `${selected.providerId}::${selected.modelId}` : '';

  return (
    <div className="model-selector">
      <label htmlFor={id} className="model-selector__label">
        <Cpu size={16} aria-hidden="true" />
        {t('ai.models.selector.label')}
      </label>
      <select
        id={id}
        className="model-selector__select"
        value={selectedValue}
        disabled={!canSelect || selectMutation.isPending || available.length === 0}
        onChange={(event) => {
          const [providerId, modelId] = event.currentTarget.value.split('::');
          if (providerId && modelId) selectMutation.mutate({ providerId, modelId });
        }}
      >
        {selectedValue === '' ? <option value="">{t('ai.models.selector.none')}</option> : null}
        {available.map((model) => (
          <option key={`${model.providerId}::${model.modelId}`} value={`${model.providerId}::${model.modelId}`}>
            {model.displayName} — {model.providerDisplayName}
          </option>
        ))}
      </select>
      {available.length === 0 ? (
        <small className="form-note">{t('ai.models.selector.empty')}</small>
      ) : null}
      {selectMutation.isError ? (
        <small className="form-error">{t('ai.models.selector.failed')}</small>
      ) : null}
    </div>
  );
};
