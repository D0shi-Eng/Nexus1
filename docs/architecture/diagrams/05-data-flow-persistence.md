# Diagram 5 — Data Flow & Persistence

**Scope:** one representative write path (create/transition an obligation-bound task) and one export path, both as implemented.

```mermaid
sequenceDiagram
  autonumber
  actor User as User (WebView2)
  participant UI as React UI (react-query)
  participant IPC as Tauri IPC boundary
  participant CMD as presentation/commands.rs
  participant SVC as Application service
  participant AUTH as authorization_service
  participant REPO as Infrastructure repository
  participant DB as SQLite (nexus-local.db)
  participant AUD as audit_service
  participant ANCHOR as audit-anchor.json
  participant DLG as Native save dialog
  participant FS as User-chosen file

  User->>UI: clicks "Create task" / "Approve"
  UI->>IPC: invoke('create_remediation_task', payload)
  IPC->>CMD: typed command (serde validation)
  CMD->>SVC: create_remediation_task(state, input)
  SVC->>AUTH: require_permission(TASK, CREATE)
  AUTH->>DB: active session + role + permission query
  DB-->>AUTH: allowed / denied (fail closed)
  SVC->>DB: BEGIN transaction
  SVC->>REPO: insert task (tenant_id, version = 1)
  REPO->>DB: INSERT (tenant-scoped)
  SVC->>AUD: append_audit_event(TASK_CREATED, actor, tenant, payload)
  AUD->>DB: INSERT audit_events (prev_hash + SHA-256)
  SVC->>DB: COMMIT
  SVC-->>CMD: DTO (safe projection)
  CMD-->>IPC: Result<DTO, SafeError>
  IPC-->>UI: JSON response
  UI->>UI: invalidate query cache and refetch
  UI->>IPC: get_tasks / get_dashboard
  IPC->>UI: fresh tenant-scoped rows

  Note over UI,DB: Reads follow the same authorization gate;<br/>UI never queries SQLite directly.

  User->>UI: clicks "Export CSV"
  UI->>DLG: native save dialog (dialog:allow-save)
  DLG-->>UI: absolute destination path
  UI->>IPC: invoke('export_report', {format, targetPath})
  IPC->>SVC: validate extension + destination policy
  SVC->>DB: build report from tenant-scoped rows
  SVC->>FS: atomic write, bounded size
  SVC->>AUD: append_audit_event(REPORT_EXPORTED)
  AUD->>DB: INSERT audit event
  UI->>IPC: verify_audit_chain
  AUD->>DB: recompute SHA-256 chain
  AUD->>ANCHOR: compare head + event count
  AUD-->>UI: chain state + anchor state
```

## Notes that keep the diagram truthful

- **One transaction, one audit event.** Business writes and their audit event commit together; an audit failure aborts the operation.
- **`SafeError` is a closed vocabulary.** Internal errors and sensitive details are not rendered into the UI; the UI receives a stable code.
- **Reads are gated too.** The diagram does not imply a public data access path; every `get_*` command calls `require_permission`.
- **Export destination policy is real.** The backend rejects relative paths, application-data directories, and unsafe destinations; writes are bounded and recorded in audit.
- **Anchor update is an explicit verification step.** `verify_audit_chain` recomputes the chain and updates the local anchor; it never silently repairs a mismatch.
- **No background writer.** The diagram has no scheduler, sync worker, or remote replication step because none exists.

## Arabic summary — ملخّص عربي

يمثّل المخطط مسار كتابة واحداً ومسار تصدير: واجهة ← IPC ← أمر مطبوع ← خدمة تطبيق ← فحص صلاحية مرتبط بالجلسة النشطة ← معاملة SQLite ← حدث تدقيق بنفس المعاملة ← استجابة آمنة ← تحديث الواجهة. القراءة تمر بالبوابة نفسها. التصدير يمر بحوار الحفظ الأصلي وسياسة وجهة صارمة وكتابة ذرّية محدودة الحجم مع حدث تدقيق. التحقق من السلسلة يعيد حساب SHA-256 ويقارن المرساة، ولا يُصلح اختلافاً بصمت. لا يوجد كاتب خلفي أو مزامنة أو جدولة.
