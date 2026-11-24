import "server-only";

/** Fixed FOR SHARE form-asset lock read shared by the public form and
 * submission adapters. The version/status scope is a closed caller-chosen
 * variant; no table, column, predicate or lock clause is accepted from a
 * caller. The row lock stays the final clause in both variants. */
export function formAssetLockSql(versioned: boolean): string {
  const scope = versioned ? " AND version=? AND status='active'" : "";
  return `SELECT id FROM marketing_assets WHERE id=? AND company_id=?${scope} FOR SHARE`;
}
