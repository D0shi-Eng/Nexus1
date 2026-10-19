// =====================================================================
// GAP-05-OPEN — تسميات الدول المعتمدة في شاشة تأسيس المؤسسة
// ---------------------------------------------------------------------
// الكتالوج نفسه يأتي من الخلف (`get_bootstrap_options`) ليبقى مصدر التحقق
// واحداً. هنا يُترجم الرمز إلى اسم مقروء فقط:
//   * الاسم من `Intl.DisplayNames` المدمج في المحرك — بلا مكتبة جديدة وبلا
//     قائمة أسماء مكتوبة يدوياً يمكن أن تنحرف عن العربية أو الإنجليزية.
//   * إن غاب المحرك أو رفض الرمز، يسقط العرض إلى الرمز نفسه — لا فشل صامت.
// =====================================================================

export interface CountryOption {
  readonly value: string;
  readonly label: string;
}

const displayNamesCache = new Map<string, Intl.DisplayNames | null>();

const displayNamesFor = (language: string): Intl.DisplayNames | null => {
  const cached = displayNamesCache.get(language);
  if (cached !== undefined) return cached;
  let names: Intl.DisplayNames | null = null;
  try {
    names = new Intl.DisplayNames([language], { type: 'region' });
  } catch {
    names = null;
  }
  displayNamesCache.set(language, names);
  return names;
};

const collatorFor = (language: string): Intl.Collator | null => {
  try {
    return new Intl.Collator(language);
  } catch {
    return null;
  }
};

/** اسم الدولة بلغة الواجهة مع رمزها: «المملكة العربية السعودية (SA)». */
export const countryLabel = (code: string, language: string): string => {
  const names = displayNamesFor(language);
  if (names !== null) {
    try {
      const name = names.of(code);
      // بعض المحركات تعيد الرمز نفسه للرمز غير المعروف — عندها لا نضيف شيئاً.
      if (name !== undefined && name !== code) return `${name} (${code})`;
    } catch {
      // رمز غير مقبول في ICU: نسقط إلى الرمز كما هو.
    }
  }
  return code;
};

/** خيارات القائمة مرتبة أبجدياً بحسب الاسم المعروض لا بحسب الرمز. */
export const countryOptions = (
  codes: readonly string[],
  language: string,
): readonly CountryOption[] => {
  const collator = collatorFor(language);
  return codes
    .map((code) => ({ value: code, label: countryLabel(code, language) }))
    .sort((first, second) =>
      collator === null ? first.value.localeCompare(second.value) : collator.compare(first.label, second.label),
    );
};
