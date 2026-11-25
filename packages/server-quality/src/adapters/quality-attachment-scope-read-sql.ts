import "server-only";
import { evidenceHead } from "./inspection-evidence-source-sql";

/** Closed-variant owner-local target-scope reads for Quality attachment facts.
 * The head routes through evidenceHead so this family keeps no inline SELECT;
 * the three admitted variants still emit the exact fixed SQL each call site
 * used before consolidation. A caller selects a variant and never supplies SQL,
 * a relation name, a predicate or an alias. Every caller prepares and executes
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
  return evidenceHead(`company_id,${columns}`, ` FROM ${table}`, ` WHERE ${predicate}`);
}
