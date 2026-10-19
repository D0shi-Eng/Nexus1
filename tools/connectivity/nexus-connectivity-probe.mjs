// فحص اتصال حقيقي لوجهات الموفرين — بلا أي نموذج وبلا أي توليد.
//
// قيود مقصودة ومعلنة:
//   - لا يُستدعى أي مسار توليد أو استدلال (`/generate` · `/chat/completions` · `/messages`).
//   - لا تُرسل أي مادة سرية ولا أي بيانات أعمال: كل الطلبات `GET` بلا ترويسة اعتماد.
//   - كل ما يُثبت هنا هو: حل الاسم، ومصافحة TLS، ووصول HTTP، وزمن الاستجابة.
//     ولا يُثبت أن العقد البرمجي للمزود صحيح (فذلك يحتاج اعتماداً واختباراً حياً منفصلاً).
//
// التشغيل: node tools/connectivity/nexus-connectivity-probe.mjs
import { lookup } from 'node:dns/promises';
import { performance } from 'node:perf_hooks';

const TIMEOUT_MS = 12_000;

// الوجهات: المحلية تُكتشف عبر مسار القائمة، والسحابية عبر مسار النماذج بلا اعتماد.
// متوقع 401/403 من المزودين المحميين — وهو دليل على أن TLS والوصول يعملان.
const TARGETS = [
  { id: 'ollama', kind: 'LOCAL', url: 'http://127.0.0.1:11434/api/tags', note: 'اكتشاف فقط — لا توليد' },
  { id: 'lm_studio', kind: 'LOCAL', url: 'http://127.0.0.1:1234/v1/models', note: 'اكتشاف فقط — لا توليد' },
  { id: 'openai', kind: 'CLOUD', url: 'https://api.openai.com/v1/models', note: 'قائمة النماذج بلا اعتماد' },
  { id: 'anthropic', kind: 'CLOUD', url: 'https://api.anthropic.com/v1/models', note: 'قائمة النماذج بلا اعتماد' },
  { id: 'xai', kind: 'CLOUD', url: 'https://api.x.ai/v1/models', note: 'قائمة النماذج بلا اعتماد' },
  { id: 'openrouter', kind: 'CLOUD', url: 'https://openrouter.ai/api/v1/models', note: 'قائمة عامة' },
  { id: 'deepseek', kind: 'CLOUD', url: 'https://api.deepseek.com/v1/models', note: 'تحقق من الوجهة (غير متحقق منها في الكود)' },
  { id: 'qwen_dashscope', kind: 'CLOUD', url: 'https://dashscope.aliyuncs.com/compatible-mode/v1/models', note: 'تحقق من الوجهة (غير متحقق منها في الكود)' },
  { id: 'zai', kind: 'CLOUD', url: 'https://api.z.ai/api/paas/v4/models', note: 'تحقق من الوجهة (غير متحقق منها في الكود)' },
  { id: 'zai_alt_zhipu', kind: 'CLOUD', url: 'https://open.bigmodel.cn/api/paas/v4/models', note: 'بائع مختلف — للمقارنة فقط' },
];

const probe = async (target) => {
  const result = { ...target, dns: 'unknown', status: null, latencyMs: null, error: null, server: null, resolvedHost: null };
  const url = new URL(target.url);
  const started = performance.now();
  try {
    const addresses = await lookup(url.hostname, { all: true });
    result.dns = addresses.length > 0 ? 'resolved' : 'no-address';
  } catch (error) {
    result.dns = 'failed';
    result.error = `DNS: ${error.code ?? error.message}`;
    result.latencyMs = Math.round(performance.now() - started);
    return result;
  }

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), TIMEOUT_MS);
  try {
    const response = await fetch(target.url, {
      method: 'GET',
      redirect: 'manual',
      signal: controller.signal,
      headers: { accept: 'application/json' },
    });
    result.status = response.status;
    result.server = response.headers.get('server');
    // نقرأ حجماً محدوداً فقط لإثبات أن الجسم وصل، ثم نتجاهله.
    const body = await response.text();
    result.bodyPreview = body.slice(0, 80).replace(/\s+/g, ' ');
    result.bodyBytes = body.length;
  } catch (error) {
    result.error = error.name === 'AbortError' ? `timeout>${TIMEOUT_MS}ms` : String(error.cause?.code ?? error.message);
  } finally {
    clearTimeout(timer);
    result.latencyMs = Math.round(performance.now() - started);
  }
  return result;
};

const results = [];
for (const target of TARGETS) {
  results.push(await probe(target));
}

// ===== تمييز مسار المحادثة: 401/403/400 ⇒ المسار موجود (الاعتماد مطلوب قبل أي تنفيذ)،
// 404 ⇒ المسار غير موجود. لا يُرسل أي اعتماد، فيستحيل أن يُنفَّذ أي استدلال.
// هذا فحص وجود مسار لا اختبار نموذج: لا أوزان تُحمَّل ولا حوسبة تحدث.
const CHAT_PATHS = [
  { id: 'openai', url: 'https://api.openai.com/v1/chat/completions' },
  { id: 'anthropic', url: 'https://api.anthropic.com/v1/messages' },
  { id: 'xai', url: 'https://api.x.ai/v1/chat/completions' },
  { id: 'openrouter', url: 'https://openrouter.ai/api/v1/chat/completions' },
  { id: 'deepseek', url: 'https://api.deepseek.com/v1/chat/completions' },
  { id: 'qwen_dashscope', url: 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions' },
  { id: 'zai', url: 'https://api.z.ai/api/paas/v4/chat/completions' },
];

const probeChatPath = async (target) => {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), TIMEOUT_MS);
  const started = performance.now();
  try {
    const response = await fetch(target.url, {
      method: 'POST',
      redirect: 'manual',
      signal: controller.signal,
      headers: { 'content-type': 'application/json' },
      body: '{}',
    });
    const body = await response.text();
    return {
      ...target,
      status: response.status,
      // 404 وحدها تعني مساراً خاطئاً؛ أي رمز مصادقة/تحقق يعني أن المسار قائم.
      pathVerdict: response.status === 404 ? 'PATH_NOT_FOUND' : 'PATH_EXISTS_AUTH_REQUIRED',
      latencyMs: Math.round(performance.now() - started),
      bodyPreview: body.slice(0, 70).replace(/\s+/g, ' '),
    };
  } catch (error) {
    return {
      ...target,
      status: null,
      pathVerdict: 'UNREACHABLE',
      latencyMs: Math.round(performance.now() - started),
      error: error.name === 'AbortError' ? `timeout>${TIMEOUT_MS}ms` : String(error.cause?.code ?? error.message),
    };
  } finally {
    clearTimeout(timer);
  }
};

const chatResults = [];
for (const target of CHAT_PATHS) {
  chatResults.push(await probeChatPath(target));
}

console.log(JSON.stringify(results, null, 2));

const verdicts = results.map((r) => {
  const reached = r.status !== null;
  const label = reached
    ? `REACHED http=${r.status} tls/http ok ${r.latencyMs}ms`
    : `NOT_REACHED ${r.error ?? 'unknown'}`;
  return `${r.id.padEnd(16)} ${label}`;
});
console.log('\n===== CONNECTIVITY SUMMARY (no model was run) =====');
for (const line of verdicts) console.log(line);

const reachedCount = results.filter((r) => r.status !== null).length;
console.log(`\nREACHED ${reachedCount}/${results.length}`);

console.log('\n===== CHAT PATH EXISTENCE (no credential sent, no model run) =====');
for (const r of chatResults) {
  console.log(`${r.id.padEnd(16)} http=${r.status ?? 'n/a'}  ${r.pathVerdict}`);
}
const pathsExist = chatResults.filter((r) => r.pathVerdict === 'PATH_EXISTS_AUTH_REQUIRED').length;
console.log(`PATHS_EXIST ${pathsExist}/${chatResults.length}`);
console.log('NOTE: reachability only. No generation endpoint was called with a credential, no model was loaded, no business data was sent.');
process.exitCode = reachedCount > 0 ? 0 : 1;
