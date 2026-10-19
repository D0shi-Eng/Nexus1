# Diagram 3 — AI Provider Integration

**Scope:** the implemented provider registry, adapters, and network policy. Only paths that exist in `apps/desktop/src-tauri/src/infrastructure/providers/` and `application/ai_provider_registry_service.rs` are drawn.

```mermaid
flowchart TB
  subgraph UI["React UI — explicit user action only"]
    PROVIDERS["Settings → AI providers<br/>add · configure · test · discover models"]
    MODELS["Settings → AI models<br/>(local model configuration)"]
    AGENT_UI["Agents / governed run UI"]
  end

  subgraph POLICY["Backend policy gates (Rust)"]
    REGISTRY["Provider registry<br/>10 fixed kinds · tenant-scoped rows"]
    DEST["Destination policy<br/>https required off-loopback · http only 127.0.0.1<br/>no credentials in URL · reject private/link-local/.local<br/>pin resolved DNS · no redirects · size + time bounds"]
    APPROVAL["Approval binding<br/>destination + model identity captured at decision"]
    TOOLPOL["Tool authorization policy<br/>allowlist per agent · risk tiers"]
    CRED["Credential vault<br/>Windows DPAPI (user scope)"]
  end

  subgraph ADAPTERS["Provider adapters"]
    OAICOMPAT["openai_compatible.rs<br/>/v1/models · /v1/chat/completions<br/>stream:false · no tool calling · no structured output<br/>fails closed with CAPABILITY_NOT_IMPLEMENTED"]
    ANTHROPIC["anthropic.rs<br/>/v1/models · /v1/messages<br/>x-api-key + anthropic-version: 2023-06-01<br/>stream:false · no tool calling"]
    OLLAMA["ollama.rs<br/>native /api/tags · /api/generate · /api/chat<br/>capabilities discovery"]
  end

  subgraph LOCAL["Local providers (loopback)"]
    OLLAMA_SRV["Ollama 127.0.0.1:11434"]
    LMSTUDIO["LM Studio 127.0.0.1:1234"]
    CUSTOM["User-entered OpenAI-compatible server"]
  end

  subgraph CLOUD["Cloud providers (HTTPS only)"]
    OPENAI["OpenAI"]
    DEEPSEEK["DeepSeek"]
    ANTH["Anthropic"]
    QWEN["Qwen / Alibaba"]
    ZAI["Z.ai"]
    XAI["xAI"]
    ROUTER["OpenRouter"]
  end

  PROVIDERS --> REGISTRY
  MODELS --> REGISTRY
  AGENT_UI --> APPROVAL
  APPROVAL --> TOOLPOL
  REGISTRY --> DEST
  CRED --> OAICOMPAT
  CRED --> ANTHROPIC
  OAICOMPAT --> DEST
  ANTHROPIC --> DEST
  OLLAMA --> DEST
  DEST --> OLLAMA_SRV
  DEST --> LMSTUDIO
  DEST --> CUSTOM
  DEST --> OPENAI
  DEST --> DEEPSEEK
  DEST --> ANTH
  DEST --> QWEN
  DEST --> ZAI
  DEST --> XAI
  DEST --> ROUTER
```

## What is real, and what is intentionally absent

- **Streaming is not implemented.** Every adapter sends `stream: false`; no SSE token stream is drawn.
- **Tool calling is implemented only in the native Ollama adapter.** The OpenAI-compatible and Anthropic adapters reject it with `CAPABILITY_NOT_IMPLEMENTED`; no tool loop is drawn for them.
- **Structured JSON output is implemented only in the native Ollama adapter** (`format: "json"` on `/api/generate`).
- **No provider is auto-enabled, and no background call exists.** The built-in Ollama row starts disabled and `UNVERIFIED`; cloud calls happen only after the user stores a key and invokes a run.
- **No credentials flow into SQLite, logs, audit events, or IPC responses.** The vault is DPAPI user-scoped; the key is read only at request time.
- **Cloud provider capability claims are documentation-level plus contract tests only.** No cloud provider has live authentication or live inference verification (see `docs/guides/ai-providers.md`).
- The diagram does **not** draw a generic "AI orchestrator cloud" or any hidden intermediary: requests go from the Rust adapter to the provider endpoint directly.

## Arabic summary — ملخّص عربي

يعرض المخطط مسار تكامل مزوّدي الذكاء الاصطناعي كما هو منفَّذ: إعداد صريح من الواجهة ← سجل المزوّدين (10 أنواع ثابتة) ← سياسة وجهة صارمة (HTTPS لغير الحلقي، والحلقي `127.0.0.1` فقط، بلا بيانات اعتماد في العنوان، وتثبيت DNS، وبلا تحويلات، وحدود حجم ومهلة) ← محوّل المحادثة. المفاتيح في خزنة DPAPI ولا تُسجَّل. البث غير منفَّذ في أي محوّل، واستدعاء الأدوات والمخرجات المهيكلة منفَّذان في مسار Ollama الأصلي فقط، والمحوّلان الآخران يفشلان مغلقين بدل ادعاء قدرة غير موجودة. لا توجد مزامنة ولا استدعاء خلفي تلقائي.
