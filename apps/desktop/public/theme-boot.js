/* إقلاع مبكر للثيم — يُنفَّذ في <head> قبل أول رسم لمنع وميض الثيم الخاطئ.
   سبب وجوده كملف خارجي: سياسة CSP في index.html تسمح بـ script-src 'self' فقط،
   فلا يجوز تضمين أي سكربت نصي داخل الصفحة.

   قائمة المعرّفات أدناه مطابقة حرفياً لـ `themeOrder` المولَّد في
   apps/desktop/src/lib/theme-order.ts، ويحرس اختبار التطابق عدم انحرافها.
   عند غياب تفضيل صريح لا تُكتب أي سمة، فيختار CSS لوحة النظام بنفسه. */
(function bootstrapThemeEarly() {
  var THEMES = ['light', 'dark', 'midnight', 'sand', 'emerald', 'contrast'];
  try {
    var stored = window.localStorage.getItem('nexus.theme');
    if (THEMES.indexOf(stored) !== -1) {
      document.documentElement.setAttribute('data-theme', stored);
    }
  } catch (error) {
    /* تخزين محجوب أو سياق مقيد: الافتراضي اتباع النظام، وهو سلوك آمن بلا كتابة. */
  }
})();
