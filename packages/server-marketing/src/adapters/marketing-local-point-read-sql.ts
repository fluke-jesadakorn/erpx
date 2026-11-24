import "server-only";

/** Closed-variant owner-local point reads for the Marketing workspace repository.
 * The three admitted variants emit byte-identical fixed queries; a caller picks
 * a variant and never supplies raw query text. Table, filter, ordering and the
 * candidate row lock stay owned here, so one fixed query backs the asset,
 * candidate and history point reads. */
export function marketingLocalPointReadSql(
  variant: "asset" | "candidate" | "history",
  lock = false,
): string {
  const table =
    variant === "asset"
      ? "marketing_assets"
      : variant === "candidate"
        ? "marketing_lead_candidates"
        : "marketing_engagement_events";
  const predicate =
    variant === "history" ? "candidate_id=?" : "id=? AND company_id=?";
  const order = variant === "history" ? " ORDER BY version" : "";
  const forUpdate = variant === "candidate" ? (lock ? " FOR UPDATE" : " ") : "";
  return `SELECT * FROM ${table} WHERE ${predicate}${order}${forUpdate}`;
}
