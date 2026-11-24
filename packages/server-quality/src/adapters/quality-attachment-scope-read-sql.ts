import "server-only";

/** Closed-variant owner-local target-scope reads for Quality attachment facts.
 * The three admitted variants emit the exact fixed SQL each call site used
 * before consolidation; a caller selects a variant and never supplies SQL, a
 * relation name, a predicate or an alias. Every caller prepares and executes
 * its own statement with its own binds and result type; no result is cached. */
export function qualityAttachmentScopeReadSql(
  variant: "legacy_hold" | "hold" | "spec",
): string {
  const columns =
    variant === "legacy_hold"
      ? "branch_id branch_id,NULL department_id,version"
      : variant === "hold"
        ? "branch_id AS branch_id,NULL AS department_id,version AS version"
        : "NULL::text branch_id,NULL::text department_id,version";
  const table =
    variant === "spec" ? "inventory_quality_specs" : "inventory_quality_holds";
  const predicate = variant === "legacy_hold" ? "id=?" : "id=? AND company_id=?";
  return `SELECT company_id,${columns} FROM ${table} WHERE ${predicate}`;
}
