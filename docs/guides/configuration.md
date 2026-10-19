# Configuration Guide

**Applies to:** NEXUS V1 `0.1.0`.

NEXUS is configured through its own UI and its local SQLite database. **The product does not read a `.env` file, a secrets file, or a remote configuration service.** There is no telemetry switch, no cloud project, and no server URL to configure.

---

## 1. Where configuration lives

| Item | Location | Notes |
|---|---|---|
| Application data | `%LOCALAPPDATA%\com.nexus.obligationos\` | production identity (`com.nexus.obligationos`) |
| Database | `...\nexus-local.db` | tenants, users, roles, business records, providers, audit |
| Audit anchor | `...\audit-anchor.json` | chain head + anchored event count |
| Local log | `...\logs\` | operational lines only; no secrets, no business payloads |
| Theme preference | WebView local storage (`nexus.theme`) | UI-only preference |
| Language | in-app switch | Arabic RTL default; English LTR |
| Isolated test container | `%LOCALAPPDATA%\com.nexus.obligationos.uatharness\` | different app identifier; never the live database |

The uninstaller does not remove the data directory. The installer and app data are intentionally separate.

## 2. Settings sections (implemented)

Open **Settings** from the bottom of the navigation. Only sections that truly exist are shown:

| Section | What it configures | Permission |
|---|---|---|
| **General** | Product identity, active runtime mode (DEMO vs PRODUCTION) | any signed-in member |
| **Appearance** | Theme preference: light, dark, midnight, sand, emerald, high contrast, system | any signed-in member |
| **Language & region** | Arabic / English, with document direction | any signed-in member |
| **AI providers** | Provider registry: add, configure, test connection, discover models, set/replace/clear credential | `integration:read`; credential writes require `integration:credential` |
| **AI models** | Select discovered models for local execution | `integration:update` |
| **Data & security** | Workspace users and roles, backups, session information | `user:read` / `backup:create` |
| **Security & password** | Change your own password (production only) | any signed-in production member |
| **About & diagnostics** | Version, mode, health, and an honest list of settings that do not exist | any signed-in member |

Anything that is not implemented is listed under **About & diagnostics** rather than presented as a fake toggle.

## 3. Language and direction

- Default: Arabic (`lang="ar"`, `dir="rtl"`).
- Switching to English flips the whole document to LTR and localizes the navigation, tables, dialogs, and errors.
- Dates and numbers follow the active locale.
- Changing language does not reload the app and does not change stored data.

## 4. Themes

Seven preferences are stored and applied: `light`, `dark`, `midnight`, `sand`, `emerald`, `contrast`, and `system`. The high-contrast theme increases border width and contrast; it was measured for text contrast ≥ 4.5:1 across the UI.

## 5. Window size

Declared minimum: **980 × 640**. Default: 1360 × 880. Resizing below the minimum is blocked by the window manager. The layout matrix verified 980×640, 1100×700, and 1280×820 in RTL and LTR across light/dark/contrast themes.

## 6. AI provider configuration

Provider rows are **tenant-scoped** and start disabled. The built-in Ollama row is seeded as `UNVERIFIED` and disabled.

Configuration steps:

1. **Settings → AI providers → Add provider**.
2. Choose one of the ten fixed kinds. The base URL is prefilled only where it is documented by the vendor; otherwise the field is empty and must be supplied by you.
3. Set the base URL. Policy: HTTPS off-loopback; plain HTTP accepted only on `127.0.0.1`; credentials inside URLs are rejected; private/link-local/descriptive hosts are rejected.
4. **Set credential** — stored via Windows DPAPI (user scope), scoped to the provider row. The key is never written to SQLite, logs, audit events, or IPC responses, and it is not shown again after saving.
5. **Test connection** → performs the real request against the validated destination and records the state.
6. **Discover models** → calls the provider's model listing path. For Qwen and Z.ai this path is not documented by the vendor; discovery may fail and the UI reports the failure instead of inventing models.
7. **Settings → AI models** → choose a discovered model for local execution.

> A credential being stored does **not** mark a provider as connected. Only a successful live test does.

### Destination policy (enforced in Rust)

| Rule | Behaviour |
|---|---|
| Non-local scheme | `https://` required |
| Loopback | `http://127.0.0.1` allowed |
| Credentials in URL | rejected |
| Private / link-local / `.local` hosts | rejected |
| DNS | resolved and pinned after validation (anti-rebinding) |
| Redirects | not followed |
| Size and time | bounded per request/response |
| Provider enablement | explicit; no automatic calls |

## 7. Backups

- Create a backup from **Settings → Data & security** (permission `backup:create`).
- Backup format: SQLite `VACUUM INTO` plus a manifest carrying schema version and SHA-256.
- Restore validates the manifest and **explicitly invalidates the audit anchor**; the anchor is re-established on the first successful chain verification after restore.
- Backups contain business data. Store them accordingly; they are not encrypted by NEXUS beyond the protection of the user's Windows profile.

## 8. Passwords and sessions

- Password policy: 12–128 characters, upper + lower + digit + symbol.
- Hashing: Argon2id.
- Production session lifetime: 12 hours.
- Lockout: 5/10/15 failed attempts → 60/300/900 seconds.
- Changing your password revokes the previous session.
- Changing language/theme never affects the session.

## 9. Environment variables used only during development

These are build/test conveniences, not product configuration:

| Variable | Used by | Purpose |
|---|---|---|
| `CARGO_TARGET_DIR` | Tauri/Rust build scripts | keep build output outside the repository |
| `RUSTFLAGS` | `build-release.ps1` | path remapping to avoid leaking builder paths into binaries |
| `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` | e2e harness | opens a local CDP port on an isolated test build only |

The product itself reads none of these for its behaviour, and none of them is a secret.

## 10. What cannot be configured in V1

There is no configuration for cloud sync, SSO/MFA, email/SMTP, update channel, analytics, external logging, or multi-tenant server URLs — because those capabilities do not exist. The UI does not pretend otherwise.
