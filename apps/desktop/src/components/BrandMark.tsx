// العلامة الرسمية لـNEXUS.
//
// المصدر الوحيد للهندسة هو الأصل المتجهي `assets/branding/nexus-icon.svg`
// (حاوية سداسية محكومة + ثلاث عقد متصلة + نواة فارغة حيث تجلس الموافقة البشرية)،
// وهو منسوخ إلى `public/branding/` ليُقدَّم كأصل محلي.
//
// لماذا صورة أصلية لا SVG مضمَّناً: المكوّنات لا تحمل SVG يدوياً في هذا المشروع
// (محروس باختبار)، والأصل يبقى مصدراً واحداً يُحدَّث في مكان واحد. ولا تُستدعى أي
// شبكة: الملف محلي داخل حزمة التطبيق.
export const BrandMark = ({ size = 34 }: { size?: number }) => (
  <img
    className="brand__image"
    src="/branding/nexus-icon.svg"
    alt=""
    width={size}
    height={size}
    aria-hidden="true"
    draggable={false}
  />
);
