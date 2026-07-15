// perm row shapes — match perm.* tables exactly (post string-grammar rebuild).

export interface Role {
  id: string;            // '<name>::<level>' e.g. 'manager::3'
  display_name: string;
  description: string | null;
  is_system: boolean;
  sort_order: number;
  parent_role_id: string | null;
  display_name_th: string | null;
  display_name_de: string | null;
  monthly_budget: number;
  head_user_id: number | null;
}

export interface Permission {
  id: string;            // '<d>:<s>:<v>[:<q>]::<effect>'
  description: string | null;
}

export interface RolePermission {
  role_id: string;
  permission_id: string;
  granted_at: string;
  granted_by: string;
}

export interface UserRole {
  user_id: number;
  role_id: string;
  granted_at: string;
}

export interface AuditEntry {
  id: number;
  kind: string;
  actor: string;
  target: unknown;
  occurred_at: string;
}
