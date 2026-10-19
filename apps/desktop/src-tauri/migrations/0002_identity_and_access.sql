PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS local_users (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  display_name TEXT NOT NULL,
  email TEXT NOT NULL,
  department_id TEXT,
  status TEXT NOT NULL CHECK (status IN ('ACTIVE', 'LOCKED', 'DISABLED')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, email)
);

CREATE TABLE IF NOT EXISTS roles (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  code TEXT NOT NULL,
  display_name TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, code)
);

CREATE TABLE IF NOT EXISTS permissions (
  id TEXT PRIMARY KEY,
  code TEXT NOT NULL UNIQUE,
  resource_type TEXT NOT NULL,
  action TEXT NOT NULL,
  classification_ceiling TEXT NOT NULL CHECK (classification_ceiling IN ('PUBLIC', 'INTERNAL', 'CONFIDENTIAL', 'RESTRICTED', 'HIGHLY_RESTRICTED')),
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS role_permissions (
  role_id TEXT NOT NULL,
  permission_id TEXT NOT NULL,
  PRIMARY KEY (role_id, permission_id),
  FOREIGN KEY (role_id) REFERENCES roles(id) ON DELETE CASCADE,
  FOREIGN KEY (permission_id) REFERENCES permissions(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS user_role_assignments (
  user_id TEXT NOT NULL,
  role_id TEXT NOT NULL,
  assigned_at TEXT NOT NULL,
  assigned_by TEXT NOT NULL,
  PRIMARY KEY (user_id, role_id),
  FOREIGN KEY (user_id) REFERENCES local_users(id) ON DELETE CASCADE,
  FOREIGN KEY (role_id) REFERENCES roles(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS local_sessions (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  user_id TEXT NOT NULL,
  issued_at TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  revoked_at TEXT,
  device_label TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (user_id) REFERENCES local_users(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_local_users_tenant ON local_users (tenant_id, status);
CREATE INDEX IF NOT EXISTS idx_sessions_tenant_user ON local_sessions (tenant_id, user_id, expires_at);
