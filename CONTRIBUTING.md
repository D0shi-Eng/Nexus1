# Contributing to NEXUS

NEXUS is a **local-first Windows 11 desktop application** for governed obligations, evidence, risk and AI-assisted operations. This document describes the toolchain, the commands and the review rules that the repository **actually uses**. Every command below is taken from the checked-in scripts (`package.json`, `apps/desktop/package.json`, `apps/desktop/src-tauri/build-release.ps1`, `.github/workflows/ci.yml`); if a command is not here, do not assume it exists.

**Repository state:** the source repository `D0shi-Eng/Nexus1` is currently **private** while the public release is prepared; no binary release has been published and no installer is distributed. External contributions are not accepted until the owner opens the repository; once it is public, this guide is the contract for contributions. The canonical English README is [`README.md`](README.md) and the canonical Arabic README is [`README.ar.md`](README.ar.md).

---

## 1. Code of conduct

Participation in this project — issues, pull requests, reviews, discussions — is governed by [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md), the **Contributor Covenant v2.1**, including its enforcement guidelines.

Report unacceptable behavior to the repository owner via <https://github.com/D0shi-Eng/Nexus1>. There is no published conduct or security mailbox, and none should be invented.

## 2. Reporting a security issue

Read [`SECURITY.md`](SECURITY.md) before you write anything.

- **Do not** open a public issue or a public pull request that describes a vulnerability, a reproduction path or an exploit. That is an explicit violation of `SECURITY.md` §3.
- Report privately: use GitHub's private reporting / security advisory flow on <https://github.com/D0shi-Eng/Nexus1>, or the private channel already used for this project (`SECURITY.md` §3). No security e-mail address is published — do not invent one, and do not expect one.
- Include: description, impact, reproduction steps on `0.1.0`, and evidence (log/screenshot) with **no real user data**.
- The stated coordination window is **90 days** from acknowledgement before any public discussion.
- Never test against real user data, never use social engineering, denial of service, or access to systems you do not own.

Security-relevant code surfaces are listed in `SECURITY.md` §2 (Rust core, IPC/capabilities/CSP, migrations `0001 → 0026`, the governed AI path, the export/backup path, and supply-chain files).

## 3. Prerequisites (measured from the repository, not estimated)

| Requirement | Value | Source |
|---|---|---|
| OS | **Windows 11 x64** — the only supported and verified platform | `README.md` → Project status |
| WebView2 | WebView2 Runtime installed | [`docs/guides/installation.md`](docs/guides/installation.md) |
| Node.js | **`>=22.0.0`** | `package.json` → `engines.node` |
| npm | **`>=10.0.0`**, pinned package manager **`npm@11.11.0`** | `package.json` → `engines.npm`, `packageManager` |
| Rust | **`rust-version = 1.77.2`** (edition 2021) + MSVC Windows Build Tools for bundling | `apps/desktop/src-tauri/Cargo.toml` |
| Bundling | NSIS target only (`bundle.targets: ["nsis"]`); **bundling requires Windows** | `apps/desktop/src-tauri/tauri.conf.json` |
| Model provider | Optional. A local loopback provider (Ollama · LM Studio · an OpenAI-compatible server). **No model is bundled** | [`docs/guides/installation.md`](docs/guides/installation.md) |

The measured build environment was node `24.14.1`, npm `11.17.0`, cargo/rustc `1.97.1` (see the *Tests and quality* section of `README.md`). Newer versions are fine; older than the `engines` / `rust-version` floors are not.

macOS and Linux are not supported for packaging: there is no bundle target for them, and no CI job builds them.

## 4. Local setup

From the repository root:

```powershell
git clone https://github.com/D0shi-Eng/Nexus1.git
cd Nexus1
npm ci
npm run dev
```

- `npm ci` installs exactly what `package-lock.json` pins; it is the command CI runs (`.github/workflows/ci.yml` → *Install dependencies (locked)*). Do not hand-edit the lockfile.
- `npm run dev` is the root script that forwards to `npm run dev --workspace=@nexus/desktop` (Vite).
- In PowerShell, if script execution policy blocks `npm.ps1`, call `npm.cmd` instead. [`docs/guides/installation.md`](docs/guides/installation.md) documents `npm.cmd install --no-audit --no-fund` as the alternative for the same step.
- To run the packaged desktop shell against the Vite dev server:

```powershell
npm run tauri --workspace=@nexus/desktop -- dev
```

(The root `desktop` script is the same workspace entry point.)

The npm workspace layout is `apps/*` and `packages/*` (`@nexus/desktop`, `@nexus/contracts`), so **the root-level scripts are the intended entry points** — they fan out with `--workspaces --if-present`.

## 5. Quality gates — all of these must pass

Run from the **repository root**:

```powershell
npm ci
npm run typecheck
npm run lint
npm run test
npm run build
```

Run from **`apps/desktop/src-tauri`**:

```powershell
cd apps\desktop\src-tauri
cargo fmt --check
cargo check --all-targets
cargo test --lib -- --test-threads=1
```

What each command really is:

| Command | What it runs | Notes |
|---|---|---|
| `npm run typecheck` | `tsc -b --pretty false` in each workspace | desktop + contracts |
| `npm run lint` | `eslint src --max-warnings=0` | **zero warnings tolerated** |
| `npm run test` | `vitest run` | measured 34 files / 202 tests |
| `npm run build` | `tsc -b && node scripts/run-vite-build.mjs` | isolates the esbuild temp dir into `apps/desktop/.build-tmp` |
| `npm run format:check` | `prettier --check .` | optional, not part of the required gates |
| `cargo fmt --check` | rustfmt only, no writes | |
| `cargo check --all-targets` | compiles lib + tests + bins | |
| `cargo test --lib -- --test-threads=1` | Rust unit tests, **serial** | serial because the suite touches SQLite files and the DPAPI credential vault |
| `node tools/e2e/nexus-uat-window.mjs <exe> --evidence=<dir>` | real WebView2 window layout matrix: 980×640 / 1100×700 / 1280×820 × RTL/LTR × light/dark/high-contrast | needs a provisioned isolated UAT container and the CDP UAT build; measures DOM geometry and frames, not screenshots alone |

Environment notes that are real and known to bite:

- On hardened machines the Rust gates may need `TMP`/`TEMP` redirected **into the workspace**; otherwise tests that create temporary directories fail because of public temp-directory restrictions, not because of the source. Do not "fix" this in code (see [`docs/guides/development.md`](docs/guides/development.md)).
- Live provider tests (Ollama / Bonsai) are `#[ignore]`d by default and are run explicitly with `--ignored` when a local model exists. **No inference runs in the default gates.**
- The Vite build prints a chunk-size warning (>500 kB). It is pre-existing and non-blocking; do not silence it to make a PR green.

## 6. Build and packaging

The release build has exactly one entry point:

```powershell
pwsh -File apps/desktop/src-tauri/build-release.ps1
```

That script:

1. resolves the repository root from its own location,
2. requires `USERPROFILE` and sets `RUSTFLAGS` with two `--remap-path-prefix` values so absolute builder paths (user name, profile folder) never land in the shipped binary,
3. runs `npx tauri build` from `apps/desktop`,
4. exits with the build's exit code.

Output: `apps/desktop/src-tauri/target/release/bundle/nsis/NEXUS_0.1.0_x64-setup.exe`.

The plain alternative sets a different target directory:

```powershell
npm run tauri --workspace=@nexus/desktop -- build
```

Because the npm `tauri` script sets `CARGO_TARGET_DIR=%LOCALAPPDATA%\NEXUS\cargo-target`, its output is `%LOCALAPPDATA%\NEXUS\cargo-target\release\bundle\nsis\`. Either way, **bundling requires Windows** (NSIS + WebView2 + MSVC), and the bundle is **unsigned**.

**The installer is never committed and never uploaded to the repository** (see [`docs/guides/installation.md`](docs/guides/installation.md)). `*.exe`, `*.msi`, `*.nsis` are ignored by `.gitignore` and rejected by the CI artifact check.

## 7. Branch policy

| Branch | Purpose |
|---|---|
| `main` | The published source line. **Changes land only through reviewed pull requests.** |
| `feature/*` and `fix/*` | Work branches for proposed changes; merged into `main` after review and green gates. |
| `hotfix/*` | Urgent corrective change on `main`, reviewed with the same gates. |

Flow: `branch → tests → review → pull request → main`. **Untested changes are never pushed to `main`.** The public repository starts from an independent clean commit; the private development history is intentionally not mirrored.

Security fixes follow the same review rule and use the private disclosure channel described in `SECURITY.md` §3; `SECURITY.md` §1 is authoritative on supported versions (no release branches, no parallel support for older versions).

## 8. Pull request requirements

- **Gates green:** `typecheck`, `lint`, `test`, `build` (npm) and `fmt --check`, `check --all-targets`, `test --lib -- --test-threads=1` (cargo) all pass locally before review. CI repeats them on `windows-latest` for pushes to `main`/`dev` and pull requests into `main`, with `contents: read` only.
- **Regression test required** for any change to authorization, authentication, sessions, or Tauri capabilities (`SECURITY.md` §4). A UI-only change still needs the UI test that covers it.
- **No secrets:** no API keys, access tokens, passwords, private keys, signing certificates, `.env` files. The CI secret scan covers the common key shapes, and it is **not** a substitute for care.
- **No build artifacts:** nothing from `dist/`, `target/`, `.build-tmp/`, no installers, no `node_modules/`, no `*.pdb`/`*.dll`. CI fails on any tracked path matching `/target/` or `^audit/uat-`.
- **No live databases:** never commit `*.db`, `*.db-wal`, `*.db-shm`, `*.sqlite*`. Test fixtures must be synthetic and declared as such.
- **No absolute developer paths:** `C:\Users\...`, host names and user names are replaced with relative paths or environment variables in code, tests, docs and commit messages (`SECURITY.md` §4).
- **No new runtime remote origin:** no CDN, no external font, no remote asset. The CSP in `tauri.conf.json` forbids it and the supply chain stays local by design.
- New or changed third-party dependencies must be reflected in `THIRD_PARTY_NOTICES.md`, and must not introduce a copyleft obligation that conflicts with distributing NEXUS under Apache-2.0.
- Keep the pull request to one logical change; update the relevant documentation in the same PR rather than in a follow-up.

## 9. License of contributions

NEXUS V1 is licensed under the **Apache License 2.0** — see [`LICENSE`](LICENSE). Copyright 2026 The NEXUS Authors.

- **Inbound = outbound:** anything you contribute is accepted under Apache-2.0, the same terms the project distributes under.
- **No CLA.** There is no Contributor License Agreement to sign, and no copyright assignment.
- **By submitting a contribution** (pull request, patch, or any code/documentation change) **you agree that your contribution is licensed under the Apache License 2.0**, and you confirm you have the right to submit it under those terms.
- Do not paste code from a project whose license is incompatible with Apache-2.0. Third-party dependency licenses are inventoried in `THIRD_PARTY_NOTICES.md`; that inventory is the reference for what is already acceptable.

## 10. Commit messages

- Subject line: imperative mood, roughly ≤ 72 characters, prefixed by type and scope, e.g. `fix(audit): detect tail truncation through the audit anchor`, `docs(delivery): record the final hardening round`, `test(evidence): reject an expiry in the past`.
- Body: **why** the change is needed, what it changes, and how it was verified (which gate or test proves it). Reference the affected module or file, not an internal ticket number nobody else can open.
- One logical change per commit; separate refactoring from behaviour changes so a reviewer can read the diff.
- Never put secrets, real data, customer identifiers, or absolute local paths in a message — commit messages are permanent.
- Do not paste raw evidence dumps or log tails into a message; summarize the result and keep the artifact on disk.

## 11. What must not be committed

The governing rule in `.gitignore` is *source only*; the same rule is enforced again in CI.

- **Internal phase reports / evidence directories.** Raw run output — `.tmp-*` (including `.tmp-uat/`), `audit/uat-*`, `audit/visual-verification/`, `phases/**/tools/`, `phases/**/*.ps1`, `phases/**/*.txt`, `phases/**/*.png` — stays on disk. These are working artifacts, not deliverable source. Do not add exceptions to `.gitignore` to force them in.
- **Screenshots.** The only published screenshots are the verified captures under `docs/images/screenshots/`. They must come from the real application running with **synthetic demo data**, must contain no personal accounts, no API keys, no company data, no personal Windows paths, and no session information. Never add a raw `.tmp-uat/` capture or a screenshot of a user database; every new screenshot needs the same review as code.
- Build output and caches: `dist/`, `target/`, `.build-tmp/`, `.tmp-build/`, `apps/desktop/src-tauri/gen/`, `coverage/`, `*.tsbuildinfo`, `.eslintcache`.
- Local databases and their journals: `*.db`, `*.db-wal`, `*.db-shm`, `*.sqlite*`.
- Logs and dumps: `*.log`, `*.dmp`, `*.mdmp`, `*.stackdump`.
- Secrets and signing material: `.env`, `*.pfx`, `*.p12`, `*.pem`, `*.key`, `*.snk`, `*.cer`, `*.crt`, `id_rsa`, `.npmrc`, `.netrc`.
- Archives and backups: `*.zip`, `*.7z`, `*.nxbackup`, `*.bak`, `*.orig`.
- Personal editor/OS state: `.vscode/`, `.idea/`, `.DS_Store`, `Thumbs.db`.

If you are unsure whether a file is sensitive: leave it out of the change and list it in the quality inventory instead of committing it.

---

<div dir="rtl">

# المساهمة في NEXUS

NEXUS تطبيق سطح مكتب **محلي أولاً** على Windows 11 لإدارة الالتزامات والأدلة والمخاطر والعمليات المدعومة بالذكاء الاصطناعي ضمن ضوابط حوكمة. هذا المستند يصف سلسلة الأدوات والأوامر وقواعد المراجعة **المستخدمة فعلاً** في المستودع؛ كل أمر أدناه مأخوذ من السكربتات المودعة (`package.json`، `apps/desktop/package.json`، `apps/desktop/src-tauri/build-release.ps1`، `.github/workflows/ci.yml`). لا تفترض وجود أمر غير مذكور هنا.

**حالة المستودع:** مستودع المصدر `D0shi-Eng/Nexus1` **خاص** حالياً أثناء تجهيز الإصدار العام؛ ولا يوجد إصدار ثنائي منشور ولا مثبّت موزَّع. ولا تُقبل المساهمات الخارجية حتى يفتح المالك المستودع؛ وعند فتحه يصبح هذا الدليل هو المرجع. و`README.md` هو README الإنجليزي المعتمد و`README.ar.md` هو العربي المعتمد.

## 1. مدونة السلوك

المشاركة في هذا المشروع — القضايا وطلبات الدمج والمراجعات والنقاشات — محكومة بـ[`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md)، وهو **Contributor Covenant الإصدار 2.1** بنصه الكامل بما فيه إرشادات التنفيذ.

أبلغ عن السلوك غير المقبول إلى مالك المستودع عبر <https://github.com/D0shi-Eng/Nexus1>. لا يوجد صندوق بريد منشور للسلوك أو للأمن، ولا يجوز اختلاق واحد.

## 2. الإبلاغ عن ثغرة أمنية

اقرأ [`SECURITY.md`](SECURITY.md) قبل أن تكتب أي شيء.

- **لا تُنشئ** قضية عامة ولا طلب دمج عاماً يصف ثغرة أو مسار إعادة إنتاج أو استغلالاً. هذا خرق صريح لـ`SECURITY.md` §3.
- أبلغ privately: استخدم مسار الإبلاغ الخاص/تنبيه الأمن في GitHub على <https://github.com/D0shi-Eng/Nexus1>، أو القناة الخاصة المستخدمة أصلاً في المشروع (`SECURITY.md` §3). لا يوجد بريد أمني منشور — لا تخترع عنواناً ولا تتوقعه.
- ضمّن التقرير: الوصف، والأثر، وخطوات إعادة الإنتاج على `0.1.0`، وأدلة (سجل/لقطة) **بلا أي بيانات مستخدم حقيقية**.
- مهلة التنسيق المعلنة **90 يوماً** من الإقرار بالاستلام قبل أي حديث عام.
- لا تختبر على بيانات مستخدم حقيقية، ولا هندسة اجتماعية، ولا حجب خدمة، ولا وصولاً إلى أنظمة لا تملكها.

أسطح الشيفرة الأمنية مسرودة في `SECURITY.md` §2 (نواة Rust، وأوامر IPC والقدرات وCSP، والمهاجرات `0001 → 0026`، ومسار الذكاء الاصطناعي المحكوم، ومسار التصدير/النسخ الاحتياطي، وملفات سلسلة التوريد).

## 3. المتطلبات المسبقة (مقيسة من المستودع، لا مقدَّرة)

| المتطلب | القيمة | المصدر |
|---|---|---|
| نظام التشغيل | **Windows 11 x64** — المنصة الوحيدة المدعومة والمتحققة | `README.ar.md` ← حالة المشروع |
| WebView2 | WebView2 Runtime مثبّت | [`docs/guides/installation.ar.md`](docs/guides/installation.ar.md) |
| Node.js | **`>=22.0.0`** | `package.json` → `engines.node` |
| npm | **`>=10.0.0`**، ومدير الحزم المثبّت **`npm@11.11.0`** | `package.json` → `engines.npm` و`packageManager` |
| Rust | **`rust-version = 1.77.2`** (إصدار 2021) + MSVC Windows Build Tools للتحزيم | `apps/desktop/src-tauri/Cargo.toml` |
| التحزيم | هدف NSIS فقط (`bundle.targets: ["nsis"]`)؛ **التحزيم يتطلب Windows** | `apps/desktop/src-tauri/tauri.conf.json` |
| مزوّد النموذج | اختياري: مزوّد محلي على loopback (Ollama · LM Studio · خادم متوافق مع OpenAI). **لا يُضمَّن أي نموذج** | [`docs/guides/installation.ar.md`](docs/guides/installation.ar.md) |

بيئة البناء المقيسة: node `24.14.1` · npm `11.17.0` · cargo/rustc `1.97.1` (راجع قسم *الاختبارات والجودة* في `README.ar.md`). الإصدارات الأحدث مقبولة؛ والأقدم من حدود `engines` و`rust-version` غير مقبولة.

لا دعم لـmacOS أو Linux في التحزيم: لا يوجد هدف حزمة لهما، ولا مهمة CI تبنيهما.

## 4. التهيئة المحلية

من جذر المستودع:

```powershell
git clone https://github.com/D0shi-Eng/Nexus1.git
cd Nexus1
npm ci
npm run dev
```

- `npm ci` يثبّت ما يثبّته `package-lock.json` بالضبط، وهو الأمر الذي تشغّله CI (`.github/workflows/ci.yml` → *Install dependencies (locked)*). لا تُعدّل ملف القفل يدوياً.
- `npm run dev` هو سكربت الجذر الذي يمرّر إلى `npm run dev --workspace=@nexus/desktop` (Vite).
- في PowerShell، إذا منعت سياسة تنفيذ السكربتات تشغيل `npm.ps1` فاستخدم `npm.cmd`. ويوثّق [`docs/guides/installation.ar.md`](docs/guides/installation.ar.md) البديل `npm.cmd install --no-audit --no-fund` لنفس الخطوة.
- لتشغيل قشرة سطح المكتب مع خادم Vite التطويري:

```powershell
npm run tauri --workspace=@nexus/desktop -- dev
```

(سكربت الجذر `desktop` هو نفس نقطة الدخول على مستوى مساحة العمل.)

بنية مساحات npm هي `apps/*` و`packages/*` (`@nexus/desktop`، `@nexus/contracts`)، لذلك **سكربتات الجذر هي نقاط الدخول المقصودة** — فهي تتفرّع بـ`--workspaces --if-present`.

## 5. بوابات الجودة — يجب أن تنجح كلها

من **جذر المستودع**:

```powershell
npm ci
npm run typecheck
npm run lint
npm run test
npm run build
```

ومن **`apps/desktop/src-tauri`**:

```powershell
cd apps\desktop\src-tauri
cargo fmt --check
cargo check --all-targets
cargo test --lib -- --test-threads=1
```

| الأمر | ما ينفّذه فعلاً | ملاحظات |
|---|---|---|
| `npm run typecheck` | `tsc -b --pretty false` في كل مساحة عمل | desktop + contracts |
| `npm run lint` | `eslint src --max-warnings=0` | **صفر تحذيرات مسموح** |
| `npm run test` | `vitest run` | المقيس: 34 ملفاً / 202 اختباراً |
| `npm run build` | `tsc -b && node scripts/run-vite-build.mjs` | يعزل مجلد esbuild المؤقت في `apps/desktop/.build-tmp` |
| `npm run format:check` | `prettier --check .` | اختياري، ليس من البوابات الإلزامية |
| `cargo fmt --check` | rustfmt للفحص فقط بلا كتابة | |
| `cargo check --all-targets` | ترجمة المكتبة والاختبارات والثنائيات | |
| `cargo test --lib -- --test-threads=1` | اختبارات Rust، **تسلسلياً** | تسلسلي لأن الجناح يمسّ ملفات SQLite وخزنة DPAPI |
| `node tools/e2e/nexus-uat-window.mjs <exe> --evidence=<dir>` | مصفوفة تخطيط نافذة WebView2 الحقيقية: 980×640 / 1100×700 / 1280×820 × RTL/LTR × فاتح/داكن/تباين عالٍ | تحتاج حاوية UAT معزولة مهيأة وبناء CDP؛ تقيس هندسة DOM والإطارات الفعلية لا لقطة شاشة واحدة |

ملاحظات بيئية حقيقية ومعلَنة:

- على الأجهزة المقيَّدة قد تحتاج بوابات Rust إلى توجيه `TMP`/`TEMP` **داخل مساحة العمل**، وإلا فشلت اختبارات تنشئ مجلدات مؤقتة بسبب قيود مجلد الحرارة العام لا بسبب المصدر. لا «تُصلح» ذلك في الشيفرة (راجع [`docs/guides/development.ar.md`](docs/guides/development.ar.md)).
- الاختبارات الحية للمزوّدين (Ollama / Bonsai) مُهمَلة افتراضياً بـ`#[ignore]` وتُشغَّل صراحةً بـ`--ignored` عند وجود نموذج محلي. **لا استنتاج في البوابات الافتراضية.**
- بناء Vite يطبع تحذير حجم chunk (>500 ك.ب). تحذير موروث وغير حاجب؛ لا تُسكِته لإرضاء المراجعة.

## 6. البناء والتحزيم

لبناء الإصدار نقطة دخول واحدة:

```powershell
pwsh -File apps/desktop/src-tauri/build-release.ps1
```

يفعل السكربت ما يلي:

1. يستنتج جذر المستودع من موقعه،
2. يشترط وجود `USERPROFILE` ويضبط `RUSTFLAGS` بقيمتَي `--remap-path-prefix` حتى لا تصل مسارات الباني المطلقة (اسم المستخدم ومجلد الملف الشخصي) إلى الثنائي الموزَّع،
3. ينفّذ `npx tauri build` من `apps/desktop`،
4. يخرج برمز خروج البناء.

الناتج: `apps/desktop/src-tauri/target/release/bundle/nsis/NEXUS_0.1.0_x64-setup.exe`.

البديل المباشر يستخدم مجلد هدف مختلفاً:

```powershell
npm run tauri --workspace=@nexus/desktop -- build
```

لأن سكربت `tauri` في npm يضبط `CARGO_TARGET_DIR=%LOCALAPPDATA%\NEXUS\cargo-target`، فإن ناتجه `%LOCALAPPDATA%\NEXUS\cargo-target\release\bundle\nsis\`. في الحالتين **التحزيم يتطلب Windows** (NSIS + WebView2 + MSVC)، والحزمة **غير موقّعة**.

**لا يُودَع المثبّت ولا يُرفع إلى المستودع** (راجع [`docs/guides/installation.ar.md`](docs/guides/installation.ar.md)). الأنماط `*.exe` و`*.msi` و`*.nsis` مُستبعدة في `.gitignore` ويرفضها فحص CI.

## 7. سياسة الفروع

| الفرع | الغرض |
|---|---|
| `main` | خط المصدر المنشور. **لا تصل التغييرات إلا عبر طلبات دمج مراجعة.** |
| `feature/*` و`fix/*` | فروع عمل للتغييرات المقترحة، تُدمج إلى `main` بعد المراجعة ونجاح البوابات. |
| `hotfix/*` | تغيير تصحيحي عاجل على `main`، يُراجع بنفس البوابات. |

المسار: `فرع → اختبارات → مراجعة → طلب دمج → main`. **لا تُدفع تغييرات غير مختبرة إلى `main`.** يبدأ المستودع العام من التزام نظيف مستقل؛ وتاريخ التطوير الخاص لا يُنسخ إليه عمداً.

إصلاحات الأمن تتبع قاعدة المراجعة نفسها وتستخدم قناة الإفصاح الخاص في `SECURITY.md` §3؛ و`SECURITY.md` §1 هو المرجع في الإصدارات المدعومة (لا فروع إصدار ولا دعم متوازٍ لنسخ قديمة).

## 8. متطلبات طلب الدمج

- **كل البوابات خضراء:** `typecheck` و`lint` و`test` و`build` (npm)، و`fmt --check` و`check --all-targets` و`test --lib -- --test-threads=1` (cargo) تنجح محلياً قبل المراجعة. وتعيد CI تشغيلها على `windows-latest` للدفع إلى `main`/`dev` وطلبات الدمج إلى `main` بصلاحية `contents: read` فقط.
- **اختبار انحدار إلزامي** لأي تغيير في التفويض أو المصادقة أو الجلسات أو قدرات Tauri (`SECURITY.md` §4). وتغيير الواجهة وحده يحتاج اختبار واجهة يغطيه.
- **بلا أسرار:** لا مفاتيح API ولا رموز وصول ولا كلمات مرور ولا مفاتيح خاصة ولا شهادات توقيع ولا ملفات `.env`. فحص CI للأسرار يغطي الأشكال الشائعة، وهو **ليس** بديلاً عن الانتباه.
- **بلا مخرجات بناء:** لا شيء من `dist/` أو `target/` أو `.build-tmp/`، ولا مثبّتات، ولا `node_modules/`، ولا `*.pdb`/`*.dll`. تفشل CI عند أي مسار متعقَّب يطابق `/target/` أو `^audit/uat-`.
- **بلا قواعد بيانات حيّة:** لا تُودِع `*.db` أو `*.db-wal` أو `*.db-shm` أو `*.sqlite*`. بيانات الاختبار يجب أن تكون تركيبية ومعلنة كذلك.
- **بلا مسارات مطوّر مطلقة:** `C:\Users\...` وأسماء المضيفين وأسماء المستخدمين تُستبدل بمسارات نسبية أو متغيرات بيئة في الشيفرة والاختبارات والتوثيق ورسائل الالتزام (`SECURITY.md` §4).
- **بلا وجهة شبكية جديدة في وقت التشغيل:** لا CDN ولا خط خارجي ولا أصل بعيد. سياسة CSP في `tauri.conf.json` تمنعه، وسلسلة التوريد تبقى محلية بحكم التصميم.
- أي تبعية خارجية جديدة أو معدّلة يجب أن تنعكس في `THIRD_PARTY_NOTICES.md`، وألا تُدخل التزاماً من نوع copyleft يتعارض مع توزيع NEXUS بترخيص Apache-2.0.
- أبقِ طلب الدمج على تغيير منطقي واحد، وحدّث التوثيق المتعلق به في الطلب نفسه لا في طلب لاحق.

## 9. ترخيص المساهمات

NEXUS V1 مرخّص بموجب **Apache License 2.0** — راجع [`LICENSE`](LICENSE). حقوق النشر 2026 The NEXUS Authors.

- **الوارد = الصادر:** أي مساهمة تُقبل بترخيص Apache-2.0، وهو نفسه ما يوزّع به المشروع.
- **لا CLA.** لا يوجد اتفاق ترخيص مساهم لتوقيعه، ولا تنازل عن حقوق النشر.
- **بتقديمك مساهمة** (طلب دمج أو رقعة أو أي تغيير في الشيفرة أو التوثيق) **فإنك توافق على أن مساهمتك مرخّصة بموجب Apache License 2.0**، وتقرّ بأنك تملك حق تقديمها بهذه الشروط.
- لا تلصق شيفرة من مشروع ترخيصه غير متوافق مع Apache-2.0. تراخيص التبعيات الخارجية محصورة في `THIRD_PARTY_NOTICES.md`، وهو المرجع لما هو مقبول أصلاً.

## 10. رسائل الالتزام

- سطر الموضوع: صيغة أمرية، بطول ≈72 حرفاً كحد أقصى، ببادئة نوع ونطاق، مثل `fix(audit): detect tail truncation through the audit anchor` أو `docs(delivery): record the final hardening round` أو `test(evidence): reject an expiry in the past`.
- المتن: **لماذا** التغيير مطلوب، وماذا يغيّر، وكيف تم التحقق منه (أي بوابة أو اختبار يثبته). أَشِر إلى الوحدة أو الملف المتأثر، لا إلى رقم تذكرة داخلية لا يستطيع غيرك فتحه.
- التغيير المنطقي الواحد في التزام واحد؛ افصل إعادة الهيكلة عن تغيير السلوك ليقدر المراجع على قراءة الفرق.
- لا تضع أسراراً ولا بيانات حقيقية ولا معرّفات عملاء ولا مسارات محلية مطلقة في رسالة الالتزام — فالرسائل دائمة.
- لا تلصق مخرجات أدلة خامة أو ذيول سجلات في الرسالة؛ لخّص النتيجة وأبقِ ملف الأدلة على القرص.

## 11. ما يجب ألا يُودَع

القاعدة الحاكمة في `.gitignore` هي *المصدر فقط*، وتُفرَض مرة أخرى في CI.

- **تقارير المراحل ومجلدات الأدلة الداخلية.** مخرجات التشغيل الخام — `.tmp-*` (ومنها `.tmp-uat/`)، و`audit/uat-*`، و`audit/visual-verification/`، و`phases/**/tools/`، و`phases/**/*.ps1`، و`phases/**/*.txt`، و`phases/**/*.png` — تبقى على القرص. هذه أدوات عمل لا مصدر قابل للتسليم. لا تُضِف استثناءات إلى `.gitignore` لإدخالها قسراً.
- **الصور.** الصور المنشورة الوحيدة هي اللقطات المتحقَّق منها تحت `docs/images/screenshots/`. ويجب أن تكون من التطبيق الحقيقي ببيانات **ديمو تركيبية**، وبلا حسابات شخصية أو مفاتيح API أو بيانات شركات أو مسارات Windows شخصية أو معلومات جلسة. لا تُضف لقطة خام من `.tmp-uat/` أو صورة لقاعدة مستخدم؛ وكل صورة جديدة تحتاج مراجعة كالشيفرة.
- مخرجات البناء والكاش: `dist/`، `target/`، `.build-tmp/`، `.tmp-build/`، `apps/desktop/src-tauri/gen/`، `coverage/`، `*.tsbuildinfo`، `.eslintcache`.
- قواعد البيانات المحلية وجورنالاتها: `*.db`، `*.db-wal`، `*.db-shm`، `*.sqlite*`.
- السجلات والانهيارات: `*.log`، `*.dmp`، `*.mdmp`، `*.stackdump`.
- الأسرار ومواد التوقيع: `.env`، `*.pfx`، `*.p12`، `*.pem`، `*.key`، `*.snk`، `*.cer`، `*.crt`، `id_rsa`، `.npmrc`، `.netrc`.
- الأرشيف والنسخ الاحتياطية: `*.zip`، `*.7z`، `*.nxbackup`، `*.bak`، `*.orig`.
- حالة المحرر ونظام التشغيل الشخصية: `.vscode/`، `.idea/`، `.DS_Store`، `Thumbs.db`.

عند الشك في حساسية ملف: استبعده من التغيير واذكره في جرد الجودة بدل إيداعه.

</div>
