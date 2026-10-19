# AI Provider Integrations

**Applies to:** NEXUS V1 `0.1.0` · **Documentation review date:** 19 October 2026.

NEXUS ships **ten fixed provider kinds**. This page states exactly what each adapter implements, how it is configured, and what has actually been verified. No provider capability is claimed beyond the evidence listed here.

> **Verification rule:** a claim is only made at the level that was actually measured. `DOCS_VERIFIED` is not `LIVE_VERIFIED`. There is **no real API key** in this repository, and no paid API call was made while building or testing V1.

---

## 1. Verification levels

| Level | Meaning | How it was established |
|---|---|---|
| `DOCS_VERIFIED` | Base URL, paths, and authentication are read in the vendor's current official documentation | Official documentation review (host named per provider) |
| `CONFIGURATION_VERIFIED` | NEXUS composes the documented full URL exactly (origin + path) | `presets.rs` unit tests |
| `CONTRACT_TESTED` | Request/response contract exercised against a **local mock HTTP server** | `*_tests.rs` with real local sockets, no keys |
| `REACHABILITY_VERIFIED` | A real connection proved the path exists (401/400, not 404) | Probe on 15 October 2026 |
| `AUTHENTICATION_VERIFIED` | A real key was accepted by the provider | **No cloud provider** |
| `LIVE_INFERENCE_VERIFIED` | Real model output returned through NEXUS | **Local Bonsai 2 only** (19 October 2026) |
| `TOOL_CALLING_LIVE_VERIFIED` | A real tool call executed by a live model | **No provider** |

`CONTRACT_TESTED` proves NEXUS understands the documented contract; it does not prove the vendor is available or that a key is valid.

## 2. Provider matrix

| # | Provider (kind) | Adapter | API type | Default base URL | Key required | Model listing | Chat completions | Streaming | Tool calling | Structured output | Verification | Known limits |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | **Ollama** (`OLLAMA`) | `providers/ollama.rs` | Native Ollama + documented `/v1` compatibility surface | `http://127.0.0.1:11434` | No (loopback) | `GET /api/tags` ✅ documented | `POST /api/generate`, `POST /api/chat` ✅ | ❌ not implemented (`stream:false`) | ✅ native `/api/chat` + `tools`; **multiple tool calls are rejected fail-closed** | ✅ `format:"json"` on `/api/generate` | `DOCS` + `CONFIG` + `CONTRACT`; **not live-verified through this adapter** | Local only; adapter relies on the native endpoints while the preset documents the `/v1` surface. |
| 2 | **LM Studio** (`LM_STUDIO`) | `providers/openai_compatible.rs` | OpenAI-compatible | `http://127.0.0.1:1234` | No (loopback) | `GET /v1/models` ✅ | `POST /v1/chat/completions` ✅ | ❌ | ❌ fail-closed | ❌ fail-closed | `DOCS` + `CONFIG` + `CONTRACT` | Port default is documented but user-changeable; verify the actual base URL. |
| 3 | **OpenAI-compatible server** (`OPENAI_COMPATIBLE`) | `providers/openai_compatible.rs` | OpenAI-compatible | **empty** — you must supply it | Yes (even for loopback; declared V1 limitation) | `GET /v1/models` | `POST /v1/chat/completions` | ❌ | ❌ | ❌ | `CONFIG` + `CONTRACT`; **`LIVE_INFERENCE_VERIFIED` with local Bonsai 2 over `127.0.0.1`** | Covers llama.cpp / Bonsai 2 / vLLM; a placeholder credential is required for unauthenticated local servers. |
| 4 | **OpenAI** (`OPENAI`) | `providers/openai_compatible.rs` | OpenAI Chat Completions | `https://api.openai.com` | Yes — `Authorization: Bearer` | `GET /v1/models` ✅ | `POST /v1/chat/completions` ✅ | ❌ | ❌ | ❌ | `DOCS` + `CONFIG` + `CONTRACT`; live **PENDING_OWNER_KEY** | Uses the documented Chat Completions surface, not the Responses API. |
| 5 | **DeepSeek** (`DEEPSEEK`) | `providers/openai_compatible.rs` | OpenAI-compatible | `https://api.deepseek.com` | Yes — Bearer | `GET /v1/models` (documented `/v1` alias) ✅ | `POST /v1/chat/completions` ✅ | ❌ | ❌ | ❌ | `DOCS` + `CONFIG` + `CONTRACT` + **`REACHABILITY_VERIFIED`**; live **PENDING_OWNER_KEY** | Model snapshots are checked strictly; error bodies are classified by HTTP status only because DeepSeek publishes no machine-readable error schema. |
| 6 | **Anthropic** (`ANTHROPIC`) | `providers/anthropic.rs` | Anthropic Messages API | `https://api.anthropic.com` | Yes — `x-api-key` + `anthropic-version: 2023-06-01` | `GET /v1/models` ✅ | `POST /v1/messages` ✅ | ❌ (vendor stream ends with `message_stop`; unused) | ❌ | ❌ | `DOCS` + `CONFIG` + `CONTRACT`; live **PENDING_OWNER_KEY** | Uses the documented legacy-but-supported `x-api-key` fallback; OAuth path not verified. |
| 7 | **Qwen / Alibaba Cloud Model Studio** (`QWEN`) | `providers/openai_compatible.rs` | OpenAI-compatible (`compatible-mode`) | `https://dashscope.aliyuncs.com` (legacy but documented) | Yes — Bearer (region-bound) | `GET /compatible-mode/v1/models` ⚠️ **not documented by vendor** (reachability only) | `POST /compatible-mode/v1/chat/completions` ✅ | ❌ | ❌ | ❌ | `DOCS` (base + chat) + `CONFIG` + `CONTRACT` + **`REACHABILITY_VERIFIED`**; live **PENDING_OWNER_KEY** | Recommended workspace-scoped host `https://{WorkspaceId}.{region}.maas.aliyuncs.com` requires a workspace id and is not bundled; model discovery may fail and is reported as failure. |
| 8 | **Z.ai** (`ZAI`) | `providers/openai_compatible.rs` | OpenAI-compatible (`v4` surface) | `https://api.z.ai` | Yes — Bearer | `GET /api/paas/v4/models` ⚠️ **not documented by vendor** (reachability only) | `POST /api/paas/v4/chat/completions` ✅ | ❌ | ❌ | ❌ | `DOCS` (base + chat) + `CONFIG` + `CONTRACT` + **`REACHABILITY_VERIFIED`**; live **PENDING_OWNER_KEY** | Version is `v4`, not `v1`; no model-listing path exists in the vendor's published OpenAPI (14 paths). Discovery may fail by design of the vendor. |
| 9 | **xAI (Grok)** (`XAI`) | `providers/openai_compatible.rs` | OpenAI Chat Completions | `https://api.x.ai` | Yes — Bearer | `GET /v1/models` ✅ | `POST /v1/chat/completions` ✅ | ❌ | ❌ | ❌ | `DOCS` + `CONFIG` + `CONTRACT`; no reachability probe; live **PENDING_OWNER_KEY** | Vendor documents Chat Completions as a predecessor of Responses; NEXUS deliberately targets the fully documented Chat Completions surface. |
| 10 | **OpenRouter** (`OPENROUTER`) | `providers/openai_compatible.rs` | OpenAI Chat Completions | `https://openrouter.ai/api` | Yes — Bearer | `GET /v1/models` ✅ | `POST /v1/chat/completions` ✅ | ❌ | ❌ | ❌ | `DOCS` + `CONFIG` + `CONTRACT`; live **PENDING_OWNER_KEY** | No request was made. Live verification needs an owner key and a spending cap. |

Legend: ✅ implemented and documented; ❌ deliberately not implemented (the adapter fails closed with `CAPABILITY_NOT_IMPLEMENTED` rather than pretending).

## 3. What is not implemented, everywhere

- **Streaming / SSE** — every adapter sends `stream: false`. The UI does not render token-by-token output.
- **Tool calling outside the native Ollama path** — OpenAI-compatible and Anthropic adapters reject it explicitly.
- **Structured outputs outside the native Ollama path** — no `response_format` is sent to OpenAI-compatible vendors because support differs; the adapter fails closed instead of guessing.
- **Automatic calls, background agents, or retries against paid services** — none exist. Every network call is an explicit user action.
- **Model weights** — not bundled. Local providers are the user's responsibility.

## 4. Configuration examples (placeholders only)

```text
Provider kind:  OpenAI-compatible
Base URL:       https://your-provider.example/v1
API key:        <YOUR_API_KEY>        # stored via Windows DPAPI, never in plain text
```

For a local OpenAI-compatible server:

```text
Provider kind:  OpenAI-compatible
Base URL:       http://127.0.0.1:8080
API key:        <LOCAL_PLACEHOLDER>   # V1 requires a credential field even for unauthenticated local servers
```

Never commit real keys to the repository, to documentation, or to screenshots.

## 5. Sources reviewed

| Provider | Official documentation reviewed |
|---|---|
| OpenAI | `developers.openai.com/api/reference` (chat completions, models list) |
| Anthropic | `platform.claude.com/docs/en/api` (messages, models, overview) |
| DeepSeek | `api-docs.deepseek.com` (create chat completion, list models) |
| Qwen / Alibaba | `alibabacloud.com/help/model-studio` (OpenAI compatibility mode) |
| Z.ai | `docs.z.ai` (quick start, chat completion, published OpenAPI) |
| xAI | `docs.x.ai` (chat completions, models) |
| OpenRouter | `openrouter.ai/docs/api-reference` (chat completion, list models) |
| Ollama | `docs.ollama.com/api` (tags, generate, chat, `/v1` compatibility) |
| LM Studio | `lmstudio.ai/docs/developer/openai-compat` |

## 6. Security properties common to all providers

- Keys are encrypted with Windows DPAPI (user scope) and read only at request time.
- Keys never appear in SQLite, logs, audit events, or IPC responses.
- Every destination passes the network policy before a connection: HTTPS off-loopback, loopback-only HTTP, no URL credentials, no private/link-local/descriptive hosts, pinned DNS, no redirects, bounded size and timeouts.
- Approval records are bound to the exact destination identity (provider, kind, endpoint, model); if that identity changes after approval, execution is refused.
- Provider rows are tenant-scoped; a credential is addressed by tenant + provider id.

## 7. Honest summary

- **Ten providers are integrated at the documentation and contract level.**
- **One local path produced real inference** (Bonsai 2 over `127.0.0.1` through the OpenAI-compatible adapter).
- **Zero cloud providers have verified authentication or live inference.**
- **Streaming, cloud tool calling, and cloud structured output are not implemented** and are not advertised.
