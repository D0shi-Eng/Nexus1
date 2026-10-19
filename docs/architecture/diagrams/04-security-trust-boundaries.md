# Diagram 4 — Security & Trust Boundaries

**Scope:** the implemented V1 trust model. Dashed edges are trust transitions; everything inside the Rust core is trusted relative to the UI.

```mermaid
flowchart TB
  subgraph HOST["Windows 11 host — the ultimate trust boundary"]
    subgraph WEBVIEW["WebView2 renderer — untrusted input surface"]
      REACT["React 19 UI<br/>no direct DB / FS / network access"]
    end

    subgraph IPC["Tauri 2 IPC boundary"]
      CAP["main-capability.json<br/>core:default · dialog:allow-open · dialog:allow-save"]
      CSP["Content-Security-Policy<br/>default-src 'self' · script-src 'self'<br/>connect-src 'self' ipc: http://ipc.localhost"]
      CMDS["117 typed commands<br/>serde validation on every payload"]
    end

    subgraph CORE["Rust core — authority"]
      AUTHN["Authentication<br/>Argon2id · lockout 5/10/15 attempts<br/>12-hour production sessions"]
      AUTHZ["Authorization<br/>session-bound RBAC · tenant scoping<br/>require_permission on every protected command"]
      DOMAIN["Domain rules<br/>task lifecycle · approval independence<br/>tool authorization · destination policy"]
      AUDIT["Audit service<br/>SHA-256 chain · append-only"]
    end

    subgraph STORE["Local storage"]
      DB[("nexus-local.db<br/>tenant_id on business rows")]
      ANCHOR[("audit-anchor.json<br/>chain head + count")]
      VAULT["DPAPI credential vault"]
      LOG["local log (no secrets)"]
    end

    OTHER["Other tenant data<br/>(same device, separate rows)"]
  end

  subgraph EXTERNAL["External boundary — optional, explicit, HTTPS"]
    CLOUDP["Cloud AI provider"]
  end

  subgraph OSADMIN["OS administrator / full device access"]
    ATTACKER["Can read and modify SQLite + anchor together"]
  end

  REACT -->|"command + payload"| CAP
  CAP --> CMDS
  CSP -.-> REACT
  CMDS --> AUTHN
  CMDS --> AUTHZ
  AUTHZ --> DOMAIN
  AUTHN --> DB
  AUTHZ --> DB
  DOMAIN --> DB
  DOMAIN --> VAULT
  DOMAIN --> AUDIT
  AUDIT --> DB
  AUDIT --> ANCHOR
  DB -. "tenant_id predicate on every query" .-> OTHER
  DOMAIN -->|"explicit user action only"| EXTERNAL
  OSADMIN -. "out of scope: full local control" .-> DB
  OSADMIN -. "out of scope" .-> ANCHOR
  DOMAIN --> LOG
```

## Boundaries stated plainly

1. **The WebView is not trusted for authority.** It cannot send identity, tenant, role, permission list, classification, or ownership. The backend derives all of them from the active session.
2. **Every protected command starts with `require_permission`.** The check joins the active user, the exact active session id, role assignments, and permissions in one query; a revoked or expired session fails closed.
3. **Tenant isolation is enforced in SQL and triggers.** Business tables carry `tenant_id`, repositories filter by the active tenant, and migration triggers reject cross-tenant re-parenting.
4. **Audit is tamper-evident, not tamper-proof.** The SHA-256 chain detects modification, deletion, reordering, actor/tenant changes, and tail truncation relative to the local anchor. A party who controls the whole device can rewrite the database **and** the anchor consistently — the product does not claim otherwise.
5. **OS compromise is out of scope.** The credential vault protects keys at rest against casual disclosure; it cannot protect against a user-level attacker who controls the same Windows account.
6. **Cloud AI is an explicit trust exit.** Only after a user configures a provider and requests a governed run does data leave the device, to the exact documented endpoint.

## Arabic summary — ملخّص عربي

حدود الثقة المنفَّذة: واجهة WebView غير موثوقة للسلطة؛ لا ترسل هوية ولا مستأجراً ولا صلاحيات، وكل أمر محمي يبدأ بفحص جلسة نشطة بعينها ودور وصلاحية في الخلف. عزل المستأجرين مفروض في SQL ومشغّلات القاعدة. سلسلة التدقيق SHA-256 كاشفة للتلاعب (تعديل/حذف/إعادة ترتيب/اقتطاع) وليست مانعة؛ ومن يملك الجهاز بالكامل يستطيع إعادة كتابة القاعدة والمرساة معاً، وهذا مصرَّح به. خزنة DPAPI تحمي المفاتيح عند السكون ولا تحمي من مهاجم يملك حساب Windows نفسه. الخروج السحابي اختياري وصريح ويحدث فقط بإجراء مستخدم.
