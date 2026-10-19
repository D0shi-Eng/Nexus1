// تنزيل خط IBM Plex Sans Arabic محلياً (ترخيص SIL OFL 1.1) وبناء `fonts.css`.
//
// الغاية: لا شبكة خطوط وقت التشغيل، ولا سقوط على Segoe UI للنص العربي.
// نُنزّل مجموعتَي `arabic` و`latin` فقط لأربعة أوزان، ونحفظ `unicode-range`
// كما هو حتى يبقى التقسيم الفرعي صحيحاً ولا يُحمَّل ملف لا يحتاجه المستخدم.
//
// التشغيل: node tools/fonts/fetch-plex-arabic.mjs
import { mkdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

const UA = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36';
const WEIGHTS = [400, 500, 600, 700];
const KEEP_SUBSETS = ['arabic', 'latin'];
const CSS_URL = `https://fonts.googleapis.com/css2?family=IBM+Plex+Sans+Arabic:wght@${WEIGHTS.join(';')}&display=swap`;
const LICENSE_URL = 'https://raw.githubusercontent.com/IBM/plex/master/LICENSE.txt';

const PUBLIC_FONTS = 'D:/NEXUS/apps/desktop/public/fonts';
const STYLES = 'D:/NEXUS/apps/desktop/src/styles';

const get = async (url, asText = false) => {
  const response = await fetch(url, { headers: { 'user-agent': UA } });
  if (!response.ok) throw new Error(`${url} -> HTTP ${response.status}`);
  return asText ? response.text() : new Uint8Array(await response.arrayBuffer());
};

const css = await get(CSS_URL, true);

// نحلّل الكتل مع تعليق المجموعة السابق لها لمعرفة أي subset نتعامل معه.
const blockPattern = /\/\*\s*([a-z-]+)\s*\*\/\s*@font-face\s*\{([\s\S]*?)\}/g;
const kept = [];
let match;
while ((match = blockPattern.exec(css)) !== null) {
  const subset = match[1];
  if (!KEEP_SUBSETS.includes(subset)) continue;
  const body = match[2];
  const weight = Number(/font-weight:\s*(\d+)/.exec(body)?.[1] ?? 0);
  const url = /url\((https:[^)]+\.woff2)\)/.exec(body)?.[1];
  const unicodeRange = /unicode-range:\s*([^;]+);/.exec(body)?.[1]?.trim();
  if (!weight || !url || !unicodeRange) continue;
  kept.push({ subset, weight, url, unicodeRange });
}

if (kept.length !== WEIGHTS.length * KEEP_SUBSETS.length) {
  throw new Error(`توقّعت ${WEIGHTS.length * KEEP_SUBSETS.length} ملفاً ووجدت ${kept.length} — أوقف للتحقق.`);
}

await mkdir(PUBLIC_FONTS, { recursive: true });

const faces = [];
const manifest = [];
for (const face of kept) {
  const fileName = `ibm-plex-sans-arabic-${face.subset}-${face.weight}.woff2`;
  const bytes = await get(face.url);
  await writeFile(join(PUBLIC_FONTS, fileName), bytes);
  manifest.push({ fileName, subset: face.subset, weight: face.weight, bytes: bytes.length });
  faces.push(
    [
      `/* ${face.subset} · ${face.weight} */`,
      '@font-face {',
      "  font-family: 'IBM Plex Sans Arabic';",
      '  font-style: normal;',
      `  font-weight: ${face.weight};`,
      // الملفات محلية فلا زمن شبكة؛ swap يمنع حجب النص إن تأخّر فك الترميز لحظياً.
      '  font-display: swap;',
      `  src: url('/fonts/${fileName}') format('woff2');`,
      `  unicode-range: ${face.unicodeRange};`,
      '}',
    ].join('\n'),
  );
}

const header = [
  '/* =============================================================================',
  '   خط NEXUS المحلي — IBM Plex Sans Arabic (ترخيص SIL OFL 1.1).',
  '   مُنزَّل ومُضمَّن في المستودع عن قصد: لا شبكة خطوط وقت التشغيل، ولا سقوط على',
  '   Segoe UI للنص العربي، فتتطابق العربية والإنجليزية في العائلة نفسها.',
  '   نص الترخيص الكامل: public/fonts/OFL.txt · وتُذكر النسبة في THIRD_PARTY_NOTICES.md.',
  '   أعِد التوليد بـ: node tools/fonts/fetch-plex-arabic.mjs',
  '   ============================================================================= */',
  '',
].join('\n');

await mkdir(STYLES, { recursive: true });
await writeFile(join(STYLES, 'fonts.css'), `${header}${faces.join('\n\n')}\n`, 'utf8');

try {
  const license = await get(LICENSE_URL, true);
  await writeFile(join(PUBLIC_FONTS, 'OFL.txt'), license, 'utf8');
  console.log('LICENSE_FETCHED=ok');
} catch (error) {
  console.log(`LICENSE_FETCHED=failed (${error.message}) — يجب إضافة نص OFL يدوياً`);
}

console.log(JSON.stringify({ faces: manifest.length, bytes: manifest.reduce((sum, f) => sum + f.bytes, 0), manifest }, null, 2));
console.log('WROTE styles/fonts.css and public/fonts/*.woff2');
