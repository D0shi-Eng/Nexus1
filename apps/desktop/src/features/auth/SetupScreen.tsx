import { useMemo, useRef, useState, type FormEvent, type RefObject } from 'react';
import { CheckCircle2, Eye, EyeOff, LoaderCircle, Play } from 'lucide-react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { countryOptions } from '../../lib/country-label';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';

// شكل بريد إلكتروني مقبول للتحقق العرضي قبل الإرسال — التحقق الفعلي يبقى خلفياً.
const EMAIL_PATTERN = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
const PASSWORD_MIN_LENGTH = 12;
const PASSWORD_MAX_LENGTH = 128;
const PASSWORD_POLICY_PATTERN = /^(?=.*[a-z])(?=.*[A-Z])(?=.*\d)(?=.*[^A-Za-z0-9]).+$/;
const ORGANIZATION_MIN_LENGTH = 3;
const ORGANIZATION_MAX_LENGTH = 120;
const ADMIN_NAME_MIN_LENGTH = 2;
const ADMIN_NAME_MAX_LENGTH = 80;
// ===== GAP-05-OPEN: قواعد حقلي الدولة والقطاع =====
// هذه نسخة عرضية مطابقة لقواعد الخلف: السلطة النهائية للخلف دائماً، ووجودها
// هنا لتقصير زمن التغذية الراجعة لا لتبرير قبول أي قيمة غير صالحة.
const INDUSTRY_MIN_LENGTH = 2;
const INDUSTRY_MAX_LENGTH = 80;
const INDUSTRY_ALLOWED_PATTERN = /^[\p{L}\p{N} &',.\-/()+]+$/u;
// العنصر النائب التاريخي ليس قطاعاً حقيقياً ولا يُقبل كإعلان جديد.
const RESERVED_INDUSTRY = 'UNSPECIFIED';

// مقترحات القطاع: تلميح كتابة فقط — القيمة المحفوظة هي ما يكتبه المستخدم.
const INDUSTRY_SUGGESTION_KEYS = [
  'technology',
  'financialServices',
  'healthcare',
  'education',
  'manufacturing',
  'retail',
  'construction',
  'logistics',
  'energy',
  'professionalServices',
  'publicSector',
  'hospitality',
] as const;

type InvalidField =
  | 'organizationName'
  | 'countryCode'
  | 'industry'
  | 'adminDisplayName'
  | 'adminEmail'
  | 'password';

interface ValidationFailure {
  readonly code: string;
  readonly field: InvalidField;
}

// تطبيع نص القطاع كما يفعل الخلف: قصّ الفراغ وتوحيد المسافات المكررة.
const normalizeIndustry = (value: string): string => value.trim().split(/\s+/).join(' ');

const validateBootstrapForm = (values: {
  readonly organizationName: string;
  readonly countryCode: string;
  readonly industry: string;
  readonly adminDisplayName: string;
  readonly adminEmail: string;
  readonly password: string;
  readonly knownCountryCodes: readonly string[];
}): ValidationFailure | null => {
  const organizationName = values.organizationName.trim();
  if (
    organizationName.length < ORGANIZATION_MIN_LENGTH
    || organizationName.length > ORGANIZATION_MAX_LENGTH
  ) {
    return { code: 'setup.validationRequired', field: 'organizationName' };
  }
  if (!values.knownCountryCodes.includes(values.countryCode)) {
    return { code: 'setup.validationCountry', field: 'countryCode' };
  }
  const industry = normalizeIndustry(values.industry);
  if (
    industry.length < INDUSTRY_MIN_LENGTH
    || industry.length > INDUSTRY_MAX_LENGTH
    || !INDUSTRY_ALLOWED_PATTERN.test(industry)
    || industry.toUpperCase() === RESERVED_INDUSTRY
  ) {
    return { code: 'setup.validationIndustry', field: 'industry' };
  }
  const adminDisplayName = values.adminDisplayName.trim();
  if (
    adminDisplayName.length < ADMIN_NAME_MIN_LENGTH
    || adminDisplayName.length > ADMIN_NAME_MAX_LENGTH
  ) {
    return { code: 'setup.validationRequired', field: 'adminDisplayName' };
  }
  if (!EMAIL_PATTERN.test(values.adminEmail.trim())) {
    return { code: 'setup.validationEmail', field: 'adminEmail' };
  }
  if (
    values.password.length < PASSWORD_MIN_LENGTH
    || values.password.length > PASSWORD_MAX_LENGTH
    || !PASSWORD_POLICY_PATTERN.test(values.password)
  ) {
    return { code: 'setup.validationPassword', field: 'password' };
  }
  return null;
};

// رموز رفض الخلف تُترجم إلى الحقل المعني لإبراز الخطأ وإعادة التركيز عليه.
const fieldForBackendError = (code: string): InvalidField | null => {
  if (code === 'BOOTSTRAP_COUNTRY_INVALID') return 'countryCode';
  if (code === 'BOOTSTRAP_INDUSTRY_INVALID') return 'industry';
  return null;
};

// شاشة التهيئة الأولى: تجربة ديمو محلية أو تهيئة مساحة إنتاج بمسؤول أول.
// كل قرار هوية يُتخذ خلفياً؛ هذه الشاشة ترسل المدخلات فقط وتعرض رموز الأخطاء الآمنة.
// وبيانات المؤسسة (الدولة والقطاع) يعلنها المستخدم هنا: لا قيمة افتراضية ولا
// استنتاج — والخلف يرفض أي قيمة لا تجتاز التحقق قبل إنشاء المؤسسة.
export const SetupScreen = () => {
  const { t, i18n } = useTranslation();
  const language = i18n.resolvedLanguage ?? 'ar';
  const queryClient = useQueryClient();
  const [organizationName, setOrganizationName] = useState('');
  const [countryCode, setCountryCode] = useState('');
  const [industry, setIndustry] = useState('');
  const [adminDisplayName, setAdminDisplayName] = useState('');
  const [adminEmail, setAdminEmail] = useState('');
  const [password, setPassword] = useState('');
  const [showPassword, setShowPassword] = useState(false);
  const [validationCode, setValidationCode] = useState<string | null>(null);
  const [invalidField, setInvalidField] = useState<InvalidField | null>(null);
  const [errorCode, setErrorCode] = useState<string | null>(null);

  const organizationField = useRef<HTMLInputElement>(null);
  const countryField = useRef<HTMLSelectElement>(null);
  const industryField = useRef<HTMLInputElement>(null);
  const adminNameField = useRef<HTMLInputElement>(null);
  const emailField = useRef<HTMLInputElement>(null);
  const passwordField = useRef<HTMLInputElement>(null);

  const fieldRefs: Record<InvalidField, RefObject<HTMLElement | null>> = {
    organizationName: organizationField,
    countryCode: countryField,
    industry: industryField,
    adminDisplayName: adminNameField,
    adminEmail: emailField,
    password: passwordField,
  };

  // كتالوج الدول يأتي من الخلف: الرمز المعروض هو نفسه الرمز الذي يُقبل.
  const optionsQuery = useQuery({
    queryKey: ['bootstrap-options', language],
    queryFn: nexusApi.getBootstrapOptions,
    staleTime: Infinity,
  });
  const knownCountryCodes = useMemo(
    () => optionsQuery.data?.countryCodes ?? [],
    [optionsQuery.data],
  );
  const countryChoices = useMemo(
    () => countryOptions(knownCountryCodes, language),
    [knownCountryCodes, language],
  );
  const industrySuggestions = useMemo(
    () => INDUSTRY_SUGGESTION_KEYS.map((key) => t(`setup.industrySuggestions.${key}`)),
    [t],
  );

  const startDemoMutation = useMutation({
    mutationFn: nexusApi.startDemoExperience,
    onSuccess: async () => {
      await queryClient.invalidateQueries();
    },
  });
  const bootstrapMutation = useMutation({
    mutationFn: nexusApi.bootstrapProduction,
    onSuccess: async () => {
      await queryClient.invalidateQueries();
    },
  });

  const focusField = (field: InvalidField): void => {
    fieldRefs[field].current?.focus();
  };

  const startDemo = async (): Promise<void> => {
    setErrorCode(null);
    try {
      await startDemoMutation.mutateAsync(undefined);
    } catch (error) {
      setErrorCode(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    }
  };

  const submit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    // حماية النقر المزدوج: لا طلب ثانٍ ما دام الأول جارياً.
    if (bootstrapMutation.isPending) return;
    setValidationCode(null);
    setInvalidField(null);
    setErrorCode(null);

    const failure = validateBootstrapForm({
      organizationName,
      countryCode,
      industry,
      adminDisplayName,
      adminEmail,
      password,
      knownCountryCodes,
    });
    if (failure !== null) {
      setValidationCode(failure.code);
      setInvalidField(failure.field);
      focusField(failure.field);
      return;
    }

    try {
      await bootstrapMutation.mutateAsync({
        organizationName: organizationName.trim(),
        countryCode,
        industry: normalizeIndustry(industry),
        adminDisplayName: adminDisplayName.trim(),
        adminEmail: adminEmail.trim(),
        password,
      });
    } catch (error) {
      const code = error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE';
      setErrorCode(code);
      // خطأ الخلف في الدولة أو القطاع يبرز الحقل المعني ويعيد التركيز عليه.
      const backendField = fieldForBackendError(code);
      if (backendField !== null) {
        setInvalidField(backendField);
        focusField(backendField);
      }
    }
  };

  return (
    <div className="auth-screen">
      <section className="auth-card panel" aria-labelledby="setup-title">
        <header className="auth-card__header">
          <span className="eyebrow">{t('product.name')}</span>
          <h1 id="setup-title">{t('setup.title')}</h1>
          <p>{t('setup.subtitle')}</p>
        </header>

        <section className="auth-choice" aria-labelledby="setup-demo-title">
          <div>
            <h2 id="setup-demo-title">{t('setup.demoTitle')}</h2>
            <p>{t('setup.demoDescription')}</p>
          </div>
          <button
            className="button button--secondary"
            type="button"
            onClick={() => { void startDemo(); }}
            disabled={startDemoMutation.isPending}
          >
            {startDemoMutation.isPending
              ? <LoaderCircle className="spin" size={16} aria-hidden="true" />
              : <Play size={16} aria-hidden="true" />}
            {t('setup.demoAction')}
          </button>
        </section>

        <form className="task-form" onSubmit={(event) => { void submit(event); }} noValidate aria-labelledby="setup-production-title">
          <div>
            <h2 id="setup-production-title">{t('setup.productionTitle')}</h2>
            <p className="auth-hint">{t('setup.productionDescription')}</p>
          </div>
          <div className="task-form__row">
            <label htmlFor="setup-organization-name">
              <span>{t('setup.organizationName')}</span>
              <input
                id="setup-organization-name"
                ref={organizationField}
                value={organizationName}
                onChange={(event) => setOrganizationName(event.currentTarget.value)}
                maxLength={ORGANIZATION_MAX_LENGTH}
                autoComplete="organization"
                required
                aria-invalid={invalidField === 'organizationName'}
              />
            </label>
            <label htmlFor="setup-admin-display-name">
              <span>{t('setup.adminDisplayName')}</span>
              <input
                id="setup-admin-display-name"
                ref={adminNameField}
                value={adminDisplayName}
                onChange={(event) => setAdminDisplayName(event.currentTarget.value)}
                maxLength={ADMIN_NAME_MAX_LENGTH}
                autoComplete="name"
                required
                aria-invalid={invalidField === 'adminDisplayName'}
              />
            </label>
          </div>
          {/* ===== GAP-05-OPEN: بيانات المؤسسة يعلنها المستخدم بنفسه ===== */}
          <div className="task-form__row">
            <label htmlFor="setup-country-code">
              <span>{t('setup.country')}</span>
              <select
                id="setup-country-code"
                ref={countryField}
                value={countryCode}
                onChange={(event) => setCountryCode(event.currentTarget.value)}
                required
                disabled={optionsQuery.isPending}
                aria-invalid={invalidField === 'countryCode'}
                aria-describedby="setup-country-hint"
              >
                <option value="">{t('setup.countryPlaceholder')}</option>
                {countryChoices.map((choice) => (
                  <option key={choice.value} value={choice.value}>{choice.label}</option>
                ))}
              </select>
              <small className="auth-hint" id="setup-country-hint">{t('setup.countryHint')}</small>
              {optionsQuery.isError
                ? <small className="auth-hint" role="alert">{t('setup.optionsUnavailable')}</small>
                : null}
            </label>
            <label htmlFor="setup-industry">
              <span>{t('setup.industry')}</span>
              <input
                id="setup-industry"
                ref={industryField}
                value={industry}
                onChange={(event) => setIndustry(event.currentTarget.value)}
                list="setup-industry-suggestions"
                maxLength={INDUSTRY_MAX_LENGTH}
                required
                aria-invalid={invalidField === 'industry'}
                aria-describedby="setup-industry-hint"
              />
              <datalist id="setup-industry-suggestions">
                {industrySuggestions.map((suggestion) => (
                  <option key={suggestion} value={suggestion} />
                ))}
              </datalist>
              <small className="auth-hint" id="setup-industry-hint">{t('setup.industryHint')}</small>
            </label>
          </div>
          <label htmlFor="setup-admin-email">
            <span>{t('setup.adminEmail')}</span>
            <input
              id="setup-admin-email"
              ref={emailField}
              type="email"
              value={adminEmail}
              onChange={(event) => setAdminEmail(event.currentTarget.value)}
              maxLength={320}
              autoComplete="email"
              required
              aria-invalid={invalidField === 'adminEmail'}
            />
          </label>
          <label htmlFor="setup-password">
            <span>{t('setup.password')}</span>
            <span className="password-field">
              <input
                id="setup-password"
                ref={passwordField}
                type={showPassword ? 'text' : 'password'}
                value={password}
                onChange={(event) => setPassword(event.currentTarget.value)}
                maxLength={PASSWORD_MAX_LENGTH}
                autoComplete="new-password"
                required
                aria-invalid={invalidField === 'password'}
              />
              <button
                className="icon-button"
                type="button"
                onClick={() => setShowPassword((visible) => !visible)}
                aria-label={showPassword ? t('setup.hidePassword') : t('setup.showPassword')}
                aria-pressed={showPassword}
              >
                {showPassword ? <EyeOff size={16} aria-hidden="true" /> : <Eye size={16} aria-hidden="true" />}
              </button>
            </span>
            <small className="auth-hint">{t('setup.passwordHint')}</small>
          </label>
          {validationCode ? <p className="form-error" role="alert">{t(validationCode)}</p> : null}
          {errorCode ? <p className="form-error" role="alert">{t(`errors.${errorCode}`, { defaultValue: t('errors.unavailable') })}</p> : null}
          <footer className="task-form__actions">
            <button className="button button--primary" type="submit" disabled={bootstrapMutation.isPending}>
              {bootstrapMutation.isPending
                ? <LoaderCircle className="spin" size={16} aria-hidden="true" />
                : <CheckCircle2 size={16} aria-hidden="true" />}
              {t('setup.submit')}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
};
