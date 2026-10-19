import { save } from '@tauri-apps/plugin-dialog';

// منفذ اختيار مسار الحفظ.
//
// الغاية: يبقى التعامل مع حوار نظام التشغيل في نقطة واحدة قابلة للاستبدال في
// الاختبار، فلا يُفتح أي حوار حقيقي أثناء الاختبارات، ولا تتفرّق معاملات الحوار
// في المكوّنات. وهو اختيار **مسار فقط**: الخلف يتحقق منه قبل أي كتابة.
export const chooseSavePath = async (options: {
  readonly defaultPath: string;
  readonly filterName: string;
  readonly extension: string;
}): Promise<string | null> => {
  const selected = await save({
    defaultPath: options.defaultPath,
    filters: [{ name: options.filterName, extensions: [options.extension] }],
  });
  return selected ?? null;
};
