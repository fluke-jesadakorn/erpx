export type TokenRow = {
  id: string; name: string; user_id: string; holding_id: string; company_id: string;
  assignment_id: string; assignment_version: number; role: string;
  assignment_scope_kind: "holding" | "branch" | "department"; assignment_scope_node_id: string | null;
  branch_id: string | null; department_id: string | null; scope_label: string;
  created_at: number; expires_at: number; revoked_at: number | null; last_used_at: number | null;
};
export type ExecutiveConnectionGrant = { id: string; role: string; version: number; scope_kind: TokenRow["assignment_scope_kind"]; scope_node_id: string | null; branch_id: string | null; modules_json: string; valid_until: number };
