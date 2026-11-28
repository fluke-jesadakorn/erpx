import "server-only";
import { marketingOwnerPointReadSql } from "./owner-point-read-sql";

/** Closed-variant owner-local point read for Marketing attachment owner facts.
 * The operational asset fact and the policy campaign fact share this one fixed
 * projection template; a caller chooses an admitted projection/table pair and
 * never supplies raw query text. */
export function attachmentOwnerFactReadSql(
  fact: "asset" | "campaign",
): string {
  const columns =
    fact === "asset"
      ? "company_id,NULL::text branch_id,NULL::text department_id,version,kind"
      : "company_id,version";
  const table = fact === "asset" ? "marketing_assets" : "marketing_campaigns";
  return marketingOwnerPointReadSql(columns, table);
}
