# Development Guide

**Applies to:** NEXUS V1 `0.1.0`.

This guide is for developers who will build, test, and change the source. Read [Architecture](../architecture/README.md) first for the system shape.

---

## 1. Repository layout

```text
apps/desktop/                 React 19 + TypeScript frontend and the Tauri/Rust application
  src/                        UI: app shell, 15 feature modules, components, lib, locales, styles
  src-tauri/                  Rust core: domain / application / infrastructure / presentation
  src-tauri/migrations/       numbered SQLite migrations 0001 → 0026
  public/                     bundled fonts (IBM Plex Sans Arabic, OFL) and theme bootstrap
packages/contracts/           shared TypeScript contract types (@nexus/contracts)
tools/                        e2e/QA harnesses, security scanner, font and theme tooling
docs/                         architecture, guides, ADRs, AI matrix, legal notices, release notes
.github/workflows/ci.yml      read-only CI (JavaScript, Rust, source hygiene)
assets/branding/              source brand assets and generated PNG/ICO exports
```

## 2. Toolchain

| Tool | Requirement |
|---|---|
| Node.js | >= 22 (CI: 24) |
| npm | >= 10; lockfile produced with `npm@11.11.0` |
| Rust | >= 1.77.2 (MSVC toolchain, `rustfmt`) |
| Tauri CLI | shipped as a dev dependency (`@tauri-apps/cli`) |
| Windows | Windows 11 with WebView2 Runtime and MSVC build tools |

## 3. First setup

```powershell
git clone https://github.com/D0shi-Eng/Nexus1.git
cd Nexus1
npm ci
npm run desktop -- dev
```

`npm ci` is required for reproducibility; do not commit a hand-edited lockfile without a concrete reason.

## 4. Commands you will actually use

| Purpose | Command |
|---|---|
| Frontend only (Vite) | `npm run dev` |
| Full desktop dev window | `npm run desktop -- dev` |
| Typecheck | `npm run typecheck` |
| Lint (zero warnings) | `npm run lint` |
| JS/TS tests | `npm run test` |
| Production frontend build | `npm run build` |
| Desktop release build | `npm run tauri --workspace=@nexus/desktop -- build` |
| Rust format check | `cargo fmt --check` (in `apps/desktop/src-tauri`) |
| Rust check (all targets) | `cargo check --all-targets` |
| Rust tests (serial) | `cargo test --lib -- --test-threads=1` |
| Secret scan | `node tools/security/secret-scan.mjs` |

> `cargo test` is run serially because some tests touch DPAPI and SQLite surfaces that are not safe to run concurrently in this environment.

## 5. Test strategy

- **Frontend:** Vitest + jsdom. Tests cover navigation contract, i18n/locale correctness, theme tokens, design-system contract, module pages, dialogs, and API error mapping.
- **Rust:** unit tests live next to the code (`*_tests.rs`) and cover authentication, authorization, tenant isolation, audit chain and anchor, provider presets/contracts, task lifecycle, approvals, evidence, controls, reports, knowledge, memory, backup, and negative security cases.
- **End-to-end:** real Tauri WebView2 driven over CDP against an **isolated container** with a separate application identifier. The harnesses cover a full production workflow, a demo workflow, UI/i18n/themes/accessibility, native save dialogs, and the window-size matrix.
- **Never test against a live user database.** The harness uses `com.nexus.obligationos.uatharness`, and screenshots use synthetic DEMO data only.

Measured on the release source tree: **202 JavaScript tests across 34 files** and **622 Rust tests passed, 10 ignored**. These numbers come from the actual runs, not from documentation claims; re-run the gates before changing them.

## 6. Architecture rules for changes

1. **Authority stays in Rust.** Never trust the frontend for identity, tenant, role, permission, classification, or ownership.
2. **Every new protected command calls `require_permission`** with an existing catalog permission; do not add permission strings without adding them to `ensure_permission_catalog` and the relevant role seeds.
3. **Audit is part of the write.** Business changes and their audit event commit in the same transaction.
4. **Domain stays framework-free.** No Tauri, SQLite, or HTTP types in `domain/`.
5. **No new network path without a destination-policy check.** HTTPS off-loopback, loopback-only HTTP, pinned DNS, no redirects, bounded size/time.
6. **The UI must state limits.** If a capability is not implemented, show that honestly instead of a non-functional control.
7. **Migrations are append-only.** Add `0027_*` rather than editing an applied migration.
8. **Bilingual coverage is part of the change.** Every user-facing string goes through `locales/ar.ts` and `locales/en.ts`, and the locale tests must stay green.

## 7. Security and hygiene rules

- No secrets in the repository: keys, tokens, passwords, `.env`, certificates, local databases, logs, or build output.
- No absolute developer paths in source, tests, docs, or commit messages.
- No remote runtime resources (CDN/fonts/scripts); the CSP forbids them by design.
- Scanner: `node tools/security/secret-scan.mjs` must report clean before any publication.
- Do not weaken a failing test to make CI pass; fix the cause.

## 8. CI

`.github/workflows/ci.yml` runs on Windows with read-only `contents: read` permission:

- **JavaScript gates:** `npm ci`, typecheck, lint, tests, build.
- **Rust gates:** `cargo fmt --check`, `cargo check --all-targets`, `cargo test --lib -- --test-threads=1`.
- **Source hygiene:** high-confidence secret patterns and a forbidden-artifact check (no tracked `.exe`, `.db`, `node_modules`, `target`, or raw audit output).

The workflow publishes no artifacts, requests no write token, and uses no provider credentials or paid APIs.

## 9. Release engineering

The release path is reproducible but **unsigned**:

```powershell
pwsh -File apps/desktop/src-tauri/build-release.ps1
```

The script remaps the builder's profile path (`--remap-path-prefix`) so absolute build paths do not leak into the executable. Verify any produced binary before distributing it; the project does not distribute installers and has no code-signing certificate.

## 10. Documentation rules

- Every factual claim in docs must be traceable to source. If it cannot be, remove it.
- Diagrams are Mermaid source files under `docs/architecture/diagrams/`; do not use fabricated screenshots or marketing drawings.
- Screenshots must come from the real application with synthetic DEMO data, never from a user's database.
- Keep the Arabic and English documents equivalent in facts and coverage; translation may change wording but not claims.

## 11. Contribution flow

1. Open an issue describing the change (or discuss it with the maintainer first).
2. Branch from `main` using `feature/*` or `fix/*`.
3. Implement, add tests, update both language versions of affected docs.
4. Run every gate in §4 locally and `node tools/security/secret-scan.mjs`.
5. Open a pull request and describe what was verified and what was not.

`SECURITY.md` and `CONTRIBUTING.md` contain the authoritative policies. Security fixes should follow the private disclosure process rather than a public PR.
