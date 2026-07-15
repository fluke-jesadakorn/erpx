// Session shape. Department and authority level are derived on demand
// from the permission list / role-id suffix via lib/perm/grammar.

export interface PermSession {
  user: {
    id: number;
    name: string;
    role: string;            // role-id, e.g. 'manager::3'
  };
  permissions: string[];     // full perm-id list including '::effect' suffix
}

export type { PermSession as PermSessionEnvelope };
