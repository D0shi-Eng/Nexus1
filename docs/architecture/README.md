# NEXUS Architecture

**Version 0.1.0** · Windows 11 x64 · Tauri 2 · Rust · React 19 · TypeScript · SQLite

This document describes the architecture that is actually implemented in `apps/desktop`. Every statement is traceable to a file in this repository. Conceptual material that is not built is labelled as such and is never presented as product behaviour.

- العربية: [README.ar.md](README.ar.md)
- Diagrams (Mermaid source, independently reviewable): [`diagrams/`](diagrams/)
- Domain context vocabulary: [`domain-context-map.md`](domain-context-map.md)

---

## 1. Shape of the application

NEXUS is a **local-first modular monolith**. One Tauri desktop process embeds:

| Layer | Location | Responsibility |
|---|---|---|
| Presentation | `src-tauri/src/presentation/` | 117 typed `#[tauri::command]` functions; converts frontend payloads into application calls and application results into safe DTOs. |
| Application | `src-tauri/src/application/` | Use cases and policy: authentication, authorization, obligations, evidence, tasks, approvals, controls, agents, skills, tools, knowledge, memory, plans, reports, audit, backup, providers, dashboard, attention, health. |
| Domain | `src-tauri/src/domain/` | Framework-free models and rules: task lifecycle, provider registry, tool authorization, truth layer, report definitions, error vocabulary. |
| Infrastructure | `src-tauri/src/infrastructure/` | SQLite database and repositories, provider adapters, DPAPI credential vault, local logger, report renderers, export writer. |
| Frontend | `apps/desktop/src/` | React 19 UI, 15 navigation modules, bilingual RTL/LTR, 7 themes, react-query data access over IPC, shared contracts from `packages/contracts`. |

The dependency direction is inward: `presentation → application → domain`, with `infrastructure` implementing storage and external I/O for the application layer. The domain layer does not depend on Tauri, SQLite, or the network.

## 2. Trust and authority model

- **The WebView holds no authority.** It cannot send identity, tenant, roles, permissions, classification, or ownership. `authorization_service::require_permission` resolves these from the active backend session for every protected command.
- **Session-bound checks.** The permission query joins the active user, the exact active session id, `revoked_at`, `expires_at`, role assignments, and permissions. Revoking a session takes effect on the next command.
- **Tenant isolation.** Business tables carry `tenant_id`; repositories filter by the active tenant, and SQLite triggers reject cross-tenant re-parenting on evidence relationships.
- **IPC surface is minimal.** `capabilities/main-capability.json` grants only `core:default`, `dialog:allow-open`, and `dialog:allow-save`. There is no shell, filesystem, HTTP plugin, or database access from the frontend.
- **CSP is local-only:** `default-src 'self'; img-src 'self' data: asset:; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src 'self' ipc: http://ipc.localhost;`.

## 3. Persistence

- One SQLite database (`nexus-local.db`) in `%LOCALAPPDATA%\com.nexus.obligationos` for the production identity, or in the isolated `com.nexus.obligationos.uatharness` container used by tests.
- **26 numbered migrations** (`0001 → 0026`) are applied at start-up; the schema version is recorded in `schema_migrations`.
- Core tables include `tenants`, `organizations`, `local_users`, `local_sessions`, `roles`, `permissions`, `regulatory_sources`, `obligations`, `evidence_items`, `tasks`, `approvals`, `risks`, `controls`, `audit_events`, `ai_provider_registry`, `ai_model_registry`, `agents`, `skills`, `tools`, `knowledge` and memory tables.
- Backups use `VACUUM INTO` with a manifest (schema version + SHA-256), and restore invalidates the audit anchor explicitly before re-anchoring.

## 4. Audit

- Each event stores the previous event hash and a SHA-256 over the event payload; `verify_audit_chain` recomputes the whole chain and compares it with `audit-anchor.json` (head hash + event count).
- Detects: event modification, deletion, reordering, actor/tenant change, and tail truncation.
- **Does not detect** a party who rewrites the database and the anchor together, and does not protect against a device administrator. The product states this limitation explicitly instead of claiming tamper-proofing.
- Audit events are written inside the same transaction as the business change; an audit failure aborts the operation.

## 5. AI provider integration

- Ten fixed provider kinds: Ollama, LM Studio, OpenAI-compatible, OpenAI, DeepSeek, Anthropic, Qwen, Z.ai, xAI, OpenRouter.
- Adapters: native Ollama (`/api/tags`, `/api/generate`, `/api/chat`), OpenAI-compatible (`/v1/models`, `/v1/chat/completions`), Anthropic Messages (`/v1/models`, `/v1/messages`).
- **Streaming is not implemented** (all requests use `stream: false`). Tool calling and structured JSON output are implemented only in the native Ollama adapter; other adapters fail closed with `CAPABILITY_NOT_IMPLEMENTED`.
- Credentials are encrypted with Windows DPAPI (user scope) and never stored in SQLite, logs, audit events, or IPC responses.
- Destination policy: HTTPS required off-loopback; HTTP only for `127.0.0.1`; no credentials in URLs; private/link-local/descriptive hosts rejected; DNS pinned after validation; no redirects; bounded request/response size and timeouts.
- No provider is enabled by default, and no call happens without an explicit user action.

## 6. Reports and export

- Six local report definitions: `OBLIGATION_STATUS`, `RISK_REGISTER`, `EVIDENCE_REGISTER`, `CONTROL_EFFECTIVENESS`, `TASK_WORKLOAD`, `AUDIT_ACTIVITY`.
- Rows are capped at 500 per section; the report declares `totalRows` and `truncated` instead of silently cutting data.
- Export formats: CSV (RFC 4180 escaping) and Markdown, through the native save dialog, with atomic bounded writes, destination policy, and one audit event per export.
- Every report carries provenance and states that it is a local operational summary, **not a legal compliance assertion**.

## 7. Bilingual and presentation layer

- Arabic is the default direction (`dir="rtl"`, `lang="ar"`); English switches the whole document to LTR without reload.
- Seven theme preferences: light, dark, midnight, sand, emerald, high contrast, and system.
- Windows are validated at the declared minimum 980×640 and at 1100×700 and 1280×820, in both directions and all measured themes.
- Accessibility: keyboard focus trap in dialogs, Escape restoration, screen-reader names measured with NVDA on the shipped build.

## 8. Deliberate non-goals in V1

Cloud sync, multi-device replication, SSO/MFA, live regulatory feeds, semantic/vector retrieval, PDF ingestion, risk authoring, automated control testing, background agent execution, signed binaries, and auto-update are **not implemented**. They are not drawn in the diagrams and are not claimed in the documentation.

## 9. Diagram index

| # | Diagram | File |
|---|---|---|
| 1 | Architecture Overview | [diagrams/01-architecture-overview.md](diagrams/01-architecture-overview.md) |
| 2 | Regulatory Workflow | [diagrams/02-regulatory-workflow.md](diagrams/02-regulatory-workflow.md) |
| 3 | AI Provider Integration | [diagrams/03-ai-provider-integration.md](diagrams/03-ai-provider-integration.md) |
| 4 | Security & Trust Boundaries | [diagrams/04-security-trust-boundaries.md](diagrams/04-security-trust-boundaries.md) |
| 5 | Data Flow & Persistence | [diagrams/05-data-flow-persistence.md](diagrams/05-data-flow-persistence.md) |
| 6 | Application Module Map | [diagrams/06-application-module-map.md](diagrams/06-application-module-map.md) |

Each diagram is written as Mermaid source so it can be reviewed in a diff and rendered directly by GitHub, and each one is accompanied by a written explanation of the implementation it represents.
