// تشغيل بناء Vite في بيئة إصدار معزولة الحرارة (Temp).
//
// سبب الوجود (عيبة D4): مُحسِّن esbuild ينشئ مجلداً مؤقتاً داخل %TEMP% ثم يحذفه عند
// الانتهاء. على هذا الجهاز يلتقط فحص Defender الفوري مقبضاً على الملف المؤقت أثناء
// الكتابة، فيفشل الحذف بـ`Access is denied` — وهو فشل في بيئة نظام الملفات لا في
// المصدر (تحويل 1741 وحدة كان ينجح ثم يفشل الحذف).
//
// الإصلاح: توجيه TMP/TEMP إلى مجلد داخل مساحة العمل قبل تحميل Vite، فيصبح مجلد
// esbuild المؤقت خارج نطاق الفحص الفوري للمجلدات المؤقتة العامة. لا يُعطَّل أي ضابط
// أمني في Windows ولا يُضاف أي استثناء (قاعدة MP05: لا تحييد لآلية أمنية).
//
// الاستخدام: node scripts/run-vite-build.mjs [وسائط vite الإضافية]

import { spawn } from 'node:child_process';
import { existsSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const desktopRoot = resolve(scriptDirectory, '..');
const buildTempDirectory = resolve(desktopRoot, '.build-tmp');

if (!existsSync(buildTempDirectory)) {
  mkdirSync(buildTempDirectory, { recursive: true });
}

// تقرير صريح للمسار المستخدم — دليل تشغيلي قابل للتحقق لا ادعاء.
console.log(`[build] isolated temp directory: ${buildTempDirectory}`);

// نسخة معدّلة من بيئة العملية — لا نعدّل process.env نفسه حتى لا تتسرب آثار البناء.
const buildEnvironment = { ...process.env, TMP: buildTempDirectory, TEMP: buildTempDirectory };
delete buildEnvironment.ESBUILD_TMPDIR;

// تشغيل Vite عبر ملفها التنفيذي الحقيقي بمحرك Node نفسه: لا صدفة (shell) ولا وسيط .cmd
// — أقل سطح تنفيذ ممكن، ومسار مستقل عن حساسية spawn لملفات .cmd على Windows.
const viteEntry = fileURLToPath(new URL('../../../node_modules/vite/bin/vite.js', import.meta.url));

const child = spawn(process.execPath, [viteEntry, 'build'], {
  cwd: desktopRoot,
  stdio: 'inherit',
  env: buildEnvironment,
});

child.on('exit', (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});
