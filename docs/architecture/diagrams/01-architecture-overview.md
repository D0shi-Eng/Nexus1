# Diagram 1 — Architecture Overview

**Scope:** the implemented NEXUS V1 desktop architecture (source: `apps/desktop`).
**Rendered by:** GitHub Mermaid.

```mermaid
flowchart TB
  subgraph UI["WebView2 — React 19 / TypeScript (apps/desktop/src)"]
    direction TB
    SHELL["Application shell<br/>15 navigation modules · RTL/LTR · 7 themes"]
    FEATURES["Feature pages<br/>overview · attention · obligations · regulatory<br/>risks · evidence · tasks · controls · agents<br/>knowledge · reports · audit · organization<br/>integrations · settings"]
    STATE["State & contracts<br/>react-query · @nexus/contracts (zod types)"]
    SHELL --> FEATURES --> STATE
  end

  subgraph BOUNDARY["Tauri 2 IPC boundary"]
    direction TB
    COMMANDS["117 typed commands<br/>presentation/commands.rs"]
    CAPS["main-capability.json<br/>core:default · dialog open/save only"]
    CSP["CSP: default-src 'self'<br/>no remote script/style/connect"]
  end

  subgraph CORE["Rust core (apps/desktop/src-tauri/src)"]
    direction TB
    PRES["Presentation<br/>DTO mapping · SafeError"]
    APP["Application services<br/>authentication · authorization · obligations · tasks<br/>approvals · evidence · controls · reports · audit<br/>agents · skills · tools · knowledge · memory · plans<br/>providers · backup · health · dashboard · attention"]
    DOMAIN["Domain<br/>models · errors · task lifecycle · provider registry<br/>tool authorization · truth · reports"]
    INFRA["Infrastructure<br/>SQLite repositories · providers adapters · DPAPI vault<br/>local logger · report renderers"]
    PRES --> APP --> DOMAIN
    APP --> INFRA
    DOMAIN --> INFRA
  end

  subgraph DATA["Local persistence"]
    DB[("nexus-local.db<br/>SQLite · 26 numbered migrations<br/>tenant-scoped tables")]
    ANCHOR[("audit-anchor.json<br/>chain head + event count")]
    LOGS["local log file<br/>no secrets · no business payloads"]
  end

  subgraph SECRETS["OS-protected secrets"]
    DPAPI["Windows DPAPI<br/>AI provider credentials"]
  end

  subgraph AI["AI provider boundary — explicit user action only"]
    LOCAL["Local loopback providers<br/>Ollama · LM Studio · OpenAI-compatible"]
    CLOUD["Cloud providers over HTTPS<br/>OpenAI · DeepSeek · Anthropic · Qwen<br/>Z.ai · xAI · OpenRouter"]
  end

  subgraph AUDIT["Audit & reporting"]
    CHAIN["SHA-256 hash chain<br/>append-only audit_events"]
    REPORTS["6 local report definitions<br/>CSV / Markdown export"]
  end

  STATE --> COMMANDS
  COMMANDS --> PRES
  COMMANDS --- CAPS
  COMMANDS --- CSP
  APP --> DB
  INFRA --> DB
  INFRA --> DPAPI
  APP --> CHAIN
  CHAIN --> ANCHOR
  APP --> REPORTS
  REPORTS --> DB
  APP --> LOCAL
  APP --> CLOUD
  APP --> LOGS
```

## What the diagram states — and what it deliberately omits

- Every business operation passes through one of the **117 `#[tauri::command]` functions** registered in `lib.rs`. The WebView never opens SQLite, files, or the network directly: the only Tauri capabilities granted are `core:default`, `dialog:allow-open`, and `dialog:allow-save`.
- The UI can only **request**; authority (identity, tenant, roles, permissions) is resolved in Rust. The frontend never sends or chooses a tenant.
- **AI providers are optional and explicit.** Local providers run on loopback; cloud providers are only contacted after the user configures a provider and triggers a call, subject to the destination policy. No provider is enabled by default (the seeded Ollama row starts `enabled = 0`, `connection_state = UNVERIFIED`).
- **Reporting reads** from the same tenant-scoped tables and writes only through the native save dialog; it is not a separate analytics service.
- There is **no cloud sync, no telemetry, no background network worker, and no server-side component** in V1. No such box is drawn.
- The audit anchor lives **inside the application data directory** — it is a local reference, not an external trusted timestamp. It is not drawn as a remote or hardware-backed root of trust.

## Arabic summary — ملخّص عربي

يعرض المخطط المعمارية الفعلية المنفَّذة في V1: واجهة React داخل WebView2 تتواصل عبر حدود IPC محددة (117 أمراً) مع نواة Rust مقسّمة إلى طبقات (عرض، خدمات تطبيق، نطاق، بنية تحتية)، وقاعدة SQLite محلية بهجرات مرقمة 0001–0026، وخزنة اعتماد محمية بـDPAPI، ومزوّدي ذكاء اصطناعي محليين/سحابيين لا يُستدعون إلا بإجراء صريح من المستخدم، وسلسلة تدقيق SHA-256 مع مرساة محلية. لا توجد مزامنة سحابية ولا تليمتري ولا خدمة خلفية — ولهذا لا تظهر في المخطط.
