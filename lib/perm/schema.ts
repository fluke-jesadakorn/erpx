// perm row shapes — match perm.* tables exactly (post string-grammar rebuild).

export interface Role {
  id: string;
  display_name: string;
  description: string | null;
  kind: 'hierarchy' | 'system';
  rank: number | null;
  is_system: boolean;
  sort_order: number;
  created_at: string;
  updated_at: string;
}

export interface Permission {
  id: string;            // '<d>:<s>:<v>[:<q>]::<effect>'
  description: string | null;
}

export interface RolePermission {
  role_id: string;
  role_kind: 'hierarchy' | 'system';
  permission_id: string;
  granted_at: string;
  granted_by: string;
}

export interface UserRole {
  user_id: number;
  role_id: string;
  role_kind: 'hierarchy' | 'system';
  granted_at: string;
}

export interface AuditEntry {
  id: number;
  kind: string;
  actor: string;
  target: unknown;
  occurred_at: string;
}
