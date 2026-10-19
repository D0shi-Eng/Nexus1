# Security and Privacy

**Applies to:** NEXUS V1 `0.1.0` · **Supported version:** `0.1.0` only (pre-release; no release branches).

This page describes the security properties that are implemented, the trust boundaries that remain, and how to report a vulnerability. It deliberately avoids absolute claims: NEXUS is not "unhackable", the audit chain is not "tamper-proof", and local-first does not mean the data is encrypted by the application.

---

## 1. Security architecture in one paragraph

NEXUS is a local-first Windows desktop application. The React WebView can only send typed commands over the Tauri IPC boundary; identity, tenant, roles, permissions, classification, and ownership are resolved in Rust. Business data lives in a single local SQLite database, scoped by `tenant_id` and protected by authorization checks before every protected command. Sensitive AI credentials are encrypted with Windows DPAPI (user scope). Audit events are hash-chained with SHA-256 and compared against a local anchor. AI providers are optional, explicit, and constrained by a strict destination policy. There is no telemetry, no cloud sync, and no background network activity.

## 2. Authentication and sessions

| Property | Implementation |
|---|---|
| Password hashing | Argon2id |
| Password policy | 12–128 characters, upper + lower + digit + symbol |
| Session lifetime (production) | 12 hours |
| Session binding | The permission query joins the **exact active session id**, `revoked_at`, and `expires_at` |
| Revocation | Logout, password change, logout-all, and account disable take effect on the next command |
| Lockout | 5/10/15 failed attempts → 60/300/900 seconds |
| Demo isolation | Demo identities/sessions are rejected in production mode by `authorization_service` |
| User enumeration | Unknown users are verified against a dummy Argon2id hash so timing does not reveal existence |

## 3. Authorization (RBAC) and tenant isolation

- **Permission catalog** is a fixed, seeded list of `resource:action` permissions (e.g. `obligation:create`, `approval:decide`, `user:update`, `integration:credential`).
- **Built-in production roles:** `TENANT_ADMIN` (all permissions), `CONTRIBUTOR` (read + own task transitions + evidence registration), `REVIEWER` (read + evidence verification + obligation verification + approval decisions).
- **No role escalation:** the backend refuses to grant a role whose permission set exceeds the granting user's authority; a user cannot modify their own account or roles.
- **Tenant isolation:** every business table carries `tenant_id`; repositories filter by the active tenant; SQLite triggers reject cross-tenant re-parenting on evidence relationships. A cross-tenant lookup returns a safe not-found/denied result without leaking the record.
- **Independent approvals:** the requester cannot decide their own approval (`SELF_APPROVAL_FORBIDDEN`), and the person who registered evidence cannot decide its verification state (`EVIDENCE_SELF_VERIFICATION_FORBIDDEN`). These are backend rules, not hidden buttons.
- **Fail-closed:** missing session, expired session, unknown role, or database error results in denial, never in an implicit allow.

## 4. IPC and the WebView boundary

- **117 typed commands** are the entire frontend→backend surface. Payloads are validated by serde before any business logic runs; unknown fields and invalid types are rejected.
- **Capabilities:** `core:default`, `dialog:allow-open`, `dialog:allow-save` only. No shell, no general filesystem, no HTTP plugin.
- **CSP:** `default-src 'self'`; scripts and styles are local only; `connect-src` is limited to `self`, `ipc:`, and `http://ipc.localhost`. There are no remote fonts, CDNs, or runtime `@import`s.
- **Safe errors:** the UI receives a stable error code (`SafeError`), not internal messages, SQL, or paths.
- The WebView is not trusted for authority: it cannot choose a tenant, a role, a permission list, a classification, or an owner.

## 5. Data at rest

| Item | Protection |
|---|---|
| SQLite database | Local file in the user's Windows profile; **not encrypted by NEXUS** |
| Audit anchor | Local JSON file next to the database |
| AI credentials | Windows DPAPI, user scope, per tenant + provider id |
| Logs | Operational lines only; no secrets and no business payloads |
| Backups | `VACUUM INTO` + manifest (schema version + SHA-256); protect them like the database itself |

> Full-disk encryption and OS account security are the user's responsibility. Anyone who can read the Windows profile can read the SQLite file unless additional OS-level protection (e.g. BitLocker) is in place.

## 6. Audit chain — what it does and does not prove

**What it detects:** modification of an event, deletion of an event, reordering, actor/tenant substitution, and tail truncation relative to `audit-anchor.json`. Each event stores the previous hash and a SHA-256 over its payload; verification recomputes the full chain.

**What it does not detect:** a party with full control of the device who rewrites the database **and** the anchor consistently. The anchor is local and is not an external timestamp or hardware root of trust. Verification never silently repairs a mismatch; a broken chain is reported as broken.

## 7. AI provider boundary

- No provider is enabled by default; every call is an explicit user action.
- Destination policy: HTTPS off-loopback; HTTP only on `127.0.0.1`; credentials in URLs rejected; private/link-local/descriptive hosts rejected; DNS pinned after validation (anti-rebinding); redirects not followed; bounded request/response size and timeouts.
- Keys are read from DPAPI at request time and never returned through IPC.
- Approval records bind the exact destination identity; a later change invalidates execution.
- **Data leaves the device only when the user configures a cloud provider and runs it.** The product cannot prevent a user from sending data to a provider they choose; the approval and audit trail record that decision.

## 8. Export and backup safety

- Exports go through the native Windows save dialog.
- The backend rejects relative paths, application-data directories, and unsafe destinations; writes are atomic and size-bounded; every export produces one audit event.
- Restore validates the manifest and explicitly invalidates the audit anchor before re-anchoring.

## 9. Known limitations (declared, not hidden)

- Audit anchor is local: full device control defeats it (§6).
- SQLite is not encrypted at rest by the application.
- Unsigned binaries: the V1 build is `NotSigned`; no code-signing certificate is distributed.
- No MFA, SSO, or external identity provider.
- No security certification or independent penetration test has been performed. V1 reports internal verification only.
- Some dependencies carry informational RustSec advisories that are absent from the Windows target; they are listed in `THIRD_PARTY_NOTICES.md` / release notes rather than hidden.
- Prompt-injection defences rely on the governed tool/approval model, not on a guarantee that a model cannot be manipulated.

## 10. Reporting a vulnerability

Please use **private coordinated disclosure**:

1. **Preferred:** GitHub Security Advisories ("Report a vulnerability") on the repository `D0shi-Eng/Nexus1`. This keeps the report private and creates a tracked fix.
2. Until the repository is publicly visible, use the private repository's collaboration/security channel with the maintainer; do not open a public issue or PR describing the vulnerability.
3. Include: affected version (`0.1.0`), impact, reproduction steps on a **synthetic or test** workspace, and evidence without real personal data.
4. Do not test against real user databases, do not perform social engineering, denial of service, or access systems you do not own.

**Commitment:** acknowledgement of receipt, a tracked fix, and notification when a corrective release is prepared. No bug-bounty program and no monetary commitment exists.

**No personal e-mail is published.** Do not invent or use one; use the GitHub channel above.

## 11. Secure development rules for contributors

- Never commit API keys, tokens, passwords, private keys, signing certificates, `.env` files, or databases.
- Do not add remote runtime resources (CDNs, fonts, scripts); CSP forbids them and local supply is a design requirement.
- Any change to authorization, authentication, sessions, or capabilities requires a regression test.
- Absolute developer paths (`C:\Users\...`, host names, account names) must not appear in source, tests, docs, or commit messages.
- When in doubt about a file's sensitivity, exclude it from publication and document why.
