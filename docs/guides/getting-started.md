# Getting Started and User Guide

**Applies to:** NEXUS V1 `0.1.0` on Windows 11 x64.

This is an operational guide to the application as it exists. UI labels are quoted exactly as they appear; the matching Arabic labels are given where useful. The permission named next to each step is the one the Rust backend actually checks — hiding a button is not the security control.

---

## 0. First launch: two paths

NEXUS opens on a setup screen with two explicit choices:

| Path | What it does | Data location |
|---|---|---|
| **Start demo experience** (`بدء التجربة التجريبية`) | Creates a synthetic DEMO tenant, three synthetic users, and demonstration records. Every screen is labelled **DEMO / SYNTHETIC DATA**. No sign-in is required. | Isolated demo data inside the app data directory, marked `DEMO_SYNTHETIC`. |
| **Provision production workspace** (`تأسيس مساحة عمل إنتاجية`) | Creates your organization, your first administrator, and no business records. You then sign in. | Production tenant (`PRODUCTION`). |

The demo path is the safe way to explore. The production path is what you use for real work.

---

## 1. Create the organization and first administrator (production)

Required fields:

- **Organization name** — Arabic or English, as you operate.
- **Administrator name**.
- **Country** — selected from the actual list loaded by the backend.
- **Industry** — free text, normalized (the product does not infer it).
- **Administrator email**.
- **Password** — 12–128 characters with upper case, lower case, a digit, and a symbol.

Result: one tenant, one administrator, one `TENANT_ADMIN` role holding the full permission catalog, and an `ORGANIZATION_PROVISIONED` audit event. **No business data is seeded** — an empty workspace is the honest starting state.

> The administrator role is the only role that receives every permission. Any user created later is deliberately granted less.

## 2. Sign in

- Email and password are required.
- Passwords are hashed with **Argon2id**; they are never returned by the backend and never logged.
- A production session is valid for **12 hours**.
- Repeated failures lock the account progressively: 5 failed attempts → 60 s, 10 → 300 s, 15 → 900 s.
- Signing out revokes the active session; revocation applies to the next command immediately.

## 3. Add users and assign roles

Open **Settings → Data & security** (`الإعدادات ← البيانات والأمان`). The **Workspace users** panel is visible only to members holding `user:read`.

1. Select **Create account**.
2. Enter display name, email, role (`CONTRIBUTOR`, `REVIEWER`, or `TENANT_ADMIN`), and an initial password following the same policy.
3. Hand the password to the member privately; they change it from **Settings → Security & password**.

Built-in role catalog:

| Role | Intended use | Real authority |
|---|---|---|
| `TENANT_ADMIN` | Workspace administration | All permissions, including user administration and approvals. |
| `CONTRIBUTOR` | Person doing the work | Reads across modules, `task:update`, `evidence:create`; **no** approval, user, authoring, backup, or export authority. |
| `REVIEWER` | Independent reviewer | Reads, `evidence:update`, `obligation:verify`, `approval:decide`; **no** user administration and no authoring. |

Rules enforced in the backend: no user may modify their own account or roles, no user may grant a role exceeding their own authority, and disabled users cannot sign in.

## 4. Register a regulatory source

Module: **Regulatory Intelligence** (`الذكاء التنظيمي`) → **Register regulatory source** (`تسجيل مصدر تنظيمي`). Permission: `regulatory_intelligence:create`.

Enter the authority, document title, version if known, and an optional URL. The record is stored as **user-provided**; NEXUS does not mark it official or verified, and there are no live regulatory feeds in V1.

## 5. Create an obligation linked to the source

Module: **Obligations** (`الالتزامات`) → **Create obligation** (`إنشاء التزام`). Permission: `obligation:create`.

Choose the registered source, title, description, owner, due date, priority, and applicability. Applicability is your assessment, not an AI decision. The record is created with a version and an audit event.

## 6. Create a task and assign responsibility

From the obligation detail, choose **Create task** (`إنشاء مهمة`). Permission: `task:create`.

Tasks can be created unassigned on purpose — the UI shows "Unassigned" rather than silently attributing the work to the creator. Assign with `task:assign` (normally `TENANT_ADMIN`; a contributor cannot assign work to others).

## 7. Work the task and register evidence

1. Assignee opens **Tasks**, chooses **Start work** (`بدء العمل`) → `OPEN → IN_PROGRESS`. Permission: `task:update`.
2. Open **Evidence** → **Register evidence reference** (`تسجيل دليل مرجعي`). Permission: `evidence:create`.
3. The evidence starts as **REQUIRES_REVIEW**. The registrant is recorded in `registered_by` and **cannot decide its verification state**, even through a direct IPC call (`EVIDENCE_SELF_VERIFICATION_FORBIDDEN`).

> V1 evidence is a *reference record* (label, classification, expiry, fingerprint). File upload, PDF ingestion, and OCR are not implemented.

## 8. Independent review of evidence

A member with `evidence:update` (REVIEWER or TENANT_ADMIN, and not the registrant) sets the evidence state to **VALID**, **INVALID**, or **EXPIRED**. The decision is audited with actor, previous/next state, and obligation impact.

## 9. Submit for review

The assignee chooses **Submit for review** (`طلب المراجعة`) → `IN_PROGRESS → READY_FOR_REVIEW`. The backend refuses this transition if no evidence is registered for the obligation — the rule is in Rust, not in the UI. A successful submission creates a **pending approval** bound to the current task version.

## 10. Approve or reject

The reviewer opens **What Needs Attention** and selects **Make decision** (`اتخاذ قرار`). Permission: `approval:decide`.

- **Approve** (`اعتماد`) → task moves to `APPROVED`.
- **Reject** (`رفض`) → task moves to `REJECTED`, and a reason is recorded and shown to the assignee.

The requester cannot decide their own approval (`SELF_APPROVAL_FORBIDDEN`), and duplicate decisions are rejected. The approval is bound to the resource version, so changes after submission invalidate the decision path.

## 11. Handle rejection and rework

From the task detail, the assignee sees the rejection reason and resumes with **Start work** (`REJECTED → IN_PROGRESS`), adds or updates evidence, and submits again. A new pending approval is created; the previous decision remains in history and in the audit trail.

## 12. Close and verify

- After approval, the assignee selects **Close task** (`إغلاق المهمة`) → `APPROVED → COMPLETED`. `COMPLETED` is terminal.
- A member with `obligation:verify` records the **human verification** of the obligation. Readiness coverage only counts human-verified obligations; it is a local operational estimate, never a legal compliance verdict.

## 13. Dashboard and attention queue

- **Executive Overview** (`نظرة تنفيذية`) shows readiness estimate, verified coverage, evidence completeness, open critical items, and system health. Every value is derived from tenant-scoped rows; missing data is shown as unmeasured.
- **What Needs Attention** (`ما يحتاج إلى اهتمام`) aggregates real items from obligations, evidence, risks, tasks, and approvals with filters and permitted actions.

## 14. Reports and export

Module: **Reports** (`التقارير`). Permission to read: `report:read`; to export: `report:export`.

Six reports are available: obligation status, risk register, evidence register, control effectiveness, task workload, and audit activity. Each declares its provenance, row count, truncation flag, and the disclaimer that it is **not a compliance assertion**.

Export: **Export CSV** or **Export Markdown**, through the native Windows save dialog. The backend validates the destination, refuses relative paths and application-data folders, writes atomically with a size cap, and records one audit event.

## 15. Review the audit trail

Module: **Audit** (`التدقيق`) — read-only, permission `audit:read`.

The list shows every event with actor, tenant, resource, timestamp, and hash chain. **Verify chain** recomputes SHA-256 over the entire chain and compares the head with the local anchor. A broken chain is reported as broken; the product never auto-repairs it.

## 16. Configure an AI provider (optional)

1. Open **Settings → AI providers** (`الإعدادات ← مزوّدو الذكاء الاصطناعي`).
2. Select **Add provider** and choose a kind; the base URL is prefilled only where it is documented by the vendor.
3. Select **Set credential** to store an API key. Permission: `integration:credential`. The key is encrypted with Windows DPAPI and never displayed again.
4. Select **Test connection** and **Discover models**. A provider is marked connected **only** after a successful live test.
5. Open **Settings → AI models** to select a discovered model for local execution.

Honest limits: streaming is not implemented; tool calling and structured output exist only in the native Ollama adapter; no cloud provider has been live-verified in this release. See [AI providers](ai-providers.md).

## 17. Manage local data

- Data lives in `%LOCALAPPDATA%\com.nexus.obligationos\nexus-local.db` (plus `audit-anchor.json` and logs).
- **Settings → Data & security** offers backup creation (`backup:create`) and restore (`backup:restore`). Backups use `VACUUM INTO` plus a manifest with schema version and SHA-256.
- Restore invalidates the audit anchor explicitly, then re-anchors on the first successful verification.
- The uninstaller does not delete application data. Back it up before removing the app.

## 18. Permissions at a glance

| Operation | Permission | Roles that have it by default |
|---|---|---|
| Read obligations / evidence / risks / tasks / reports / audit | `obligation:read`, `evidence:read`, `risk:read`, `task:read`, `report:read`, `audit:read` | All three roles |
| Create regulatory source | `regulatory_intelligence:create` | `TENANT_ADMIN` |
| Create obligation | `obligation:create` | `TENANT_ADMIN` |
| Update / archive obligation | `obligation:update`, `obligation:archive` | `TENANT_ADMIN` |
| Human-verify obligation | `obligation:verify` | `TENANT_ADMIN`, `REVIEWER` |
| Create / assign task | `task:create`, `task:assign` | `TENANT_ADMIN` |
| Move own task through lifecycle | `task:update` | `TENANT_ADMIN`, `CONTRIBUTOR`, `REVIEWER` |
| Register evidence | `evidence:create` | `TENANT_ADMIN`, `CONTRIBUTOR` |
| Decide evidence state | `evidence:update` | `TENANT_ADMIN`, `REVIEWER` (never the registrant) |
| Decide approval | `approval:decide` | `TENANT_ADMIN`, `REVIEWER` (never the requester) |
| Create / update / disable / link / assess controls | `control:create`, `control:update`, `control:disable`, `control:link`, `control:assess` | `TENANT_ADMIN` |
| Manage users and roles | `user:read`, `user:create`, `user:update` | `TENANT_ADMIN` |
| Create / restore backup | `backup:create`, `backup:restore` | `TENANT_ADMIN` |
| Export reports | `report:export` | `TENANT_ADMIN` |
| Configure provider credential | `integration:update`, `integration:credential` | `TENANT_ADMIN` |

## 19. What to do when something is missing

If a page shows a module as not implemented or read-only, that is the truth of V1 — for example **Risks** is a read-only register and risk authoring does not exist. Do not expect hidden functionality; the navigation deliberately keeps the module visible while stating its real scope.
