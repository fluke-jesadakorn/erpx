/** Fixed 15-column assignments projection shared by the owner snapshot sources.
 * The compact variant only removes the column-separator spaces, so every consumer
 * keeps byte-identical clause text (its JOIN/FROM prefix and alias included). */
export const assignmentsProjection = 'SELECT "id", "user_id", "role", "modules_json", "company_id", "branch_id", "valid_from", "valid_until", "revoked_at", "version", "scope_kind", "scope_node_id", "parent_id", "source", "source_id" FROM assignments';
export const assignmentsProjectionCompact = assignmentsProjection.replace(/", "/g, '","');
