# Installation and Build Guide

**Applies to:** NEXUS V1 `0.1.0` · Windows 11 x64
**Status:** source distribution only. There is **no published installer and no official binary release yet**.

This guide distinguishes four different things that are often confused:

| Term | Status in V1 |
|---|---|
| **Download source** | Available: clone the Git repository. |
| **Build from source** | Available: documented and verified in this repository. |
| **Install locally** | Available only as the unsigned NSIS package produced by your own build. |
| **Official binary release** | **Not available.** No signed installer, no GitHub Release artifact, no download page. |

> The project does not distribute unsigned executables. If you build an installer yourself, it is your local artifact and it is **not signed**.

---

## 1. Prerequisites (verified against the repository)

| Requirement | Version | Evidence |
|---|---|---|
| Windows | 11 x64 | `tauri.conf.json` targets the Windows WebView2 stack; CI runs `windows-latest`. |
| Node.js | **>= 22** (tested with 24.14.1) | `package.json` → `engines.node`. |
| npm | **>= 10**, lockfile generated with `npm@11.11.0` | `package.json` → `engines.npm`, `packageManager`. |
| Rust | **>= 1.77.2** (tested with 1.97.1) | `apps/desktop/src-tauri/Cargo.toml` → `rust-version`. |
| Rust toolchain | `x86_64-pc-windows-msvc` with `rustfmt` | CI uses `dtolnay/rust-toolchain@stable` + `rustfmt`. |
| C++ build tools | MSVC toolchain + Windows SDK | required to link Tauri/WebView2 binaries. |
| WebView2 | Evergreen runtime | included with Windows 11; otherwise install the Microsoft WebView2 Runtime. |
| NSIS | bundled by the Tauri CLI for the `nsis` bundle target | `bundle.targets: ["nsis"]`. |

Optional tools used by the QA harness (not required to build the product): `playwright-core` for CDP-driven UI tests, and NVDA for accessibility verification.

## 2. Get the source

```powershell
git clone https://github.com/D0shi-Eng/Nexus1.git
cd Nexus1
```

## 3. Install dependencies

```powershell
npm ci
```

`npm ci` installs the exact locked dependency graph from `package-lock.json` for the root, `apps/desktop`, `packages/contracts`, and the tool workspaces.

## 4. Run the frontend only (fast UI iteration)

```powershell
npm run dev
```

This starts Vite on `http://localhost:1420`. It renders the UI but **not** the Tauri backend, so desktop commands will be unavailable; use it for layout work only.

## 5. Run the real desktop application in development

```powershell
npm run desktop -- dev
```

This starts Vite and the Tauri development window (WebView2) with the Rust backend attached.

> The desktop workspace script sets `CARGO_TARGET_DIR=%LOCALAPPDATA%\NEXUS\cargo-target` so Rust build output stays outside the repository. You can override it with your own `CARGO_TARGET_DIR` if you prefer.

## 6. Quality gates (same commands CI runs)

JavaScript / TypeScript:

```powershell
npm run typecheck
npm run lint
npm run test
npm run build
```

Rust:

```powershell
cd apps/desktop/src-tauri
cargo fmt --check
cargo check --all-targets
cargo test --lib -- --test-threads=1
```

The test suite is serial by design because parts of it exercise DPAPI and SQLite surfaces. `cargo test` requires the WebView2/Tauri build dependencies to be present.

## 7. Build a release binary and NSIS installer

```powershell
# from the repository root
npm run tauri --workspace=@nexus/desktop -- build
```

To build with the path-sanitizing release script used for the internal V1 candidate:

```powershell
pwsh -File apps/desktop/src-tauri/build-release.ps1
```

Expected outputs (default target directory):

- Application: `%LOCALAPPDATA%\NEXUS\cargo-target\release\nexus_desktop.exe`
  (or `apps/desktop/src-tauri/target/release/nexus_desktop.exe` if you do not set `CARGO_TARGET_DIR`).
- Installer: `...\release\bundle\nsis\NEXUS_0.1.0_x64-setup.exe`.

The installer is **unsigned** (`NotSigned`). Windows SmartScreen may warn. The project does not provide a code-signing certificate.

## 8. Install locally (optional, your own build)

1. Build the NSIS installer as in step 7.
2. Run `NEXUS_0.1.0_x64-setup.exe`.
3. The installer places the application, an uninstaller, and legal notices (`legal/LICENSE`, `legal/NOTICE`, `legal/THIRD_PARTY_NOTICES.md`).
4. Application data is stored in `%LOCALAPPDATA%\com.nexus.obligationos`. The uninstaller does **not** delete that data directory; back it up from inside the app if you need it.

## 9. Isolated test containers

The QA harness uses a different application identifier (`com.nexus.obligationos.uatharness`) and a separate data directory, so acceptance tests never open a user's live database:

```powershell
# Build a debug harness binary (isolated identifier)
cd apps/desktop
$env:CARGO_TARGET_DIR="$env:LOCALAPPDATA\NEXUS\cargo-target"
npx tauri build --debug --no-bundle --config ..\..\..\tools\e2e\uat-harness.conf.json

# Seed and run the production workflow against the isolated container
pwsh -File tools\e2e\run-uat.ps1 -Phase production -Reset
```

These harnesses are development tools; they are not part of the product.

## 10. Troubleshooting

| Symptom | Cause / fix |
|---|---|
| `link.exe not found` | Install Visual Studio Build Tools with the C++ workload and Windows SDK. |
| WebView2 window is blank | Install/repair the Microsoft WebView2 Evergreen Runtime. |
| `cargo test` fails to write temp files | Environment restriction (e.g. low-integrity folder). Build/test from a normal user directory or set `CARGO_TARGET_DIR` and `TEMP` to a normal path. |
| npm install is not deterministic | Use `npm ci`, not `npm install`, and do not modify the lockfile without a reason. |
| Installer is flagged by SmartScreen | Expected for an unsigned build. See [Security](../guides/security.md) — trusted signing is an owner decision. |
