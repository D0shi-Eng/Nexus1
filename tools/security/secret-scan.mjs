/**
 * NEXUS publishable-snapshot secret scanner.
 *
 * الغاية: منع دخول أي سر حقيقي إلى مستودع GitHub الخاص. الفحص يعمل على مجموعة
 * الملفات القابلة للنشر فعلاً (لا على المخرجات المستبعدة)، ويعيد مواضع الإصابات
 * بلا طباعة قيمها حتى لا يتحول السجل نفسه إلى تسريب.
 *
 * Usage: node tools/security/secret-scan.mjs [--json]
 */
import { readFileSync, statSync } from 'node:fs';
import { execFileSync } from 'node:child_process';

const asJson = process.argv.includes('--json');
const ROOT = process.cwd();

// أنماط الأسرار: تُبنى لتفادي الإيجابيات الكاذبة الشائعة (هاشات التدقيق، مفاتيح ترجمة، أسماء حقول).
const RULES = [
  { id: 'private-key-block', severity: 'CRITICAL', re: /-----BEGIN (?:RSA |EC |DSA |OPENSSH |PGP )?PRIVATE KEY-----/ },
  { id: 'github-pat-classic', severity: 'CRITICAL', re: /\bghp_[A-Za-z0-9]{36}\b/ },
  { id: 'github-pat-fine', severity: 'CRITICAL', re: /\bgithub_pat_[A-Za-z0-9_]{82}\b/ },
  { id: 'github-oauth', severity: 'CRITICAL', re: /\bgho_[A-Za-z0-9]{36}\b/ },
  { id: 'github-app-token', severity: 'CRITICAL', re: /\b(?:ghu|ghs)_[A-Za-z0-9]{36}\b/ },
  { id: 'aws-access-key', severity: 'CRITICAL', re: /\b(?:AKIA|ASIA)[0-9A-Z]{16}\b/ },
  { id: 'aws-secret', severity: 'CRITICAL', re: /aws_secret_access_key\s*[:=]\s*['"][A-Za-z0-9/+=]{40}['"]/i },
  { id: 'slack-token', severity: 'HIGH', re: /\bxox[baprs]-[0-9A-Za-z-]{10,}\b/ },
  { id: 'google-api-key', severity: 'HIGH', re: /\bAIza[0-9A-Za-z_-]{35}\b/ },
  { id: 'openai-key', severity: 'CRITICAL', re: /\bsk-[A-Za-z0-9]{20,}\b/ },
  { id: 'anthropic-key', severity: 'CRITICAL', re: /\bsk-ant-[A-Za-z0-9_-]{20,}\b/ },
  { id: 'huggingface-token', severity: 'HIGH', re: /\bhf_[A-Za-z0-9]{30,}\b/ },
  { id: 'jwt', severity: 'MEDIUM', re: /\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\b/ },
  { id: 'npg-connection-string', severity: 'HIGH', re: /(?:postgres|postgresql|mysql|mongodb(?:\+srv)?|redis):\/\/[^\s:@/]+:[^\s:@/]+@/i },
  { id: 'generic-assigned-secret', severity: 'MEDIUM', re: /(?:password|passwd|secret|api[_-]?key|access[_-]?token|auth[_-]?token|client[_-]?secret)\s*[:=]\s*['"][^'"\s${}<>]{12,}['"]/i },
  { id: 'bearer-literal', severity: 'MEDIUM', re: /Authorization\s*[:=]\s*['"]?Bearer\s+[A-Za-z0-9._-]{20,}/i },
  // مسار يفضح ملف تعريف المؤلف على جهاز حقيقي
  { id: 'windows-user-profile-path', severity: 'MEDIUM', re: /[A-Za-z]:\\Users\\(?!Public\\)[A-Za-z0-9._-]+\\/ },
  { id: 'unix-home-path', severity: 'LOW', re: /(?:\/home\/|\/Users\/)(?!runner|user\b)[a-z0-9._-]+\// },
];

// استثناءات مقصودة: قيم تركيبية معلَنة، وأسماء أعمدة مخطّط، ومسارات أدوات مُلزَمة.
const ALLOW_CONTEXT = [
  /DEMO\s*\/?\s*SYNTHETIC/i,
  /example|placeholder|dummy|sample|redacted|changeme|your-|<your|xxx+/i,
  /(?:password|secret|token|api[_-]?key)\s*[:=]\s*['"]\s*['"]/i,
  /\.test\.|\.spec\.|\/tests?\//,
  /include_str!|\.sql:/,
  // قيم اختبار اصطناعية موسومة صراحةً في الشيفرة والوثائق (ليست مفاتيح صالحة).
  /\bCANARY\b/i,
  /NEVER-LEAK|do-not-leak/i,
  /Str0ng!Passw0rd|Wr0ng!Passw0rd|An0ther!Passw0rd|Memb3r!Passw0rd/,
];

const isProbablyBinary = (buf) => buf.subarray(0, 8000).includes(0);

// الفحص على **لقطة النشر الفعلية** فقط: الملفات المتعقَّبة + غير المتعقَّبة غير
// المستبعدة. لا نستخدم `git status` لأنه يعيد مسارات محذوفة من الفهرس ما زالت
// على القرص (أرشيف المراحل الداخلية) فتُلوّث الحكم بإيجابيات ليست في النشر.
const files = execFileSync('git', ['ls-files', '-co', '--exclude-standard'], { cwd: ROOT, encoding: 'utf8' })
  .split('\n')
  .filter(Boolean)
  .filter((p) => {
    try {
      return statSync(p).isFile();
    } catch {
      return false;
    }
  })
  .filter((p, i, arr) => arr.indexOf(p) === i);

const findings = [];
let scannedBytes = 0;
let skippedBinary = 0;

for (const file of files) {
  let buf;
  try {
    buf = readFileSync(file);
  } catch {
    continue;
  }
  scannedBytes += buf.length;
  if (isProbablyBinary(buf)) {
    skippedBinary += 1;
    continue;
  }
  const text = buf.toString('utf8');
  const lines = text.split(/\r?\n/);
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    if (line.length > 4000) continue;
    for (const rule of RULES) {
      if (!rule.re.test(line)) continue;
      // سياق معلَن كتركيبي/اختباري يُسقط الإيجابية الكاذبة
      const window = lines.slice(Math.max(0, i - 2), i + 3).join(' ');
      if (ALLOW_CONTEXT.some((a) => a.test(window))) continue;
      findings.push({
        rule: rule.id,
        severity: rule.severity,
        file,
        line: i + 1,
        // لا نطبع القيمة أبداً — فقط طولها ونوع المحرف الأول لإثبات وجود مطابقة.
        matchLength: (line.match(rule.re)?.[0] ?? '').length,
      });
    }
  }
}

const bySeverity = findings.reduce((acc, f) => {
  acc[f.severity] = (acc[f.severity] ?? 0) + 1;
  return acc;
}, {});

const report = {
  scanner: 'nexus-secret-scan',
  scannedAt: new Date().toISOString(),
  filesScanned: files.length,
  skippedBinary,
  scannedBytes,
  findings,
  bySeverity,
  verdict: findings.some((f) => f.severity === 'CRITICAL' || f.severity === 'HIGH') ? 'BLOCK' : (findings.length ? 'REVIEW' : 'CLEAN'),
};

if (asJson) {
  console.log(JSON.stringify(report, null, 2));
} else {
  console.log(`files scanned : ${report.filesScanned} (binary skipped: ${skippedBinary})`);
  console.log(`bytes scanned : ${scannedBytes}`);
  console.log(`findings      : ${findings.length}`, bySeverity);
  for (const f of findings) console.log(`  [${f.severity}] ${f.rule} ${f.file}:${f.line} (matchLength=${f.matchLength})`);
  console.log(`verdict       : ${report.verdict}`);
}
process.exit(report.verdict === 'CLEAN' ? 0 : 1);
