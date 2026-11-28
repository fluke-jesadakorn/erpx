import "server-only";
import {commerceFinalOwnerProjectionSources as projections} from "@erpx/procurement-server/adapters/commerce-final-owner-projection-sources";
/** Closed native projections with explicit owned columns for named Finance reads.
 * The consumer retains its supplied transaction and one-statement snapshot;
 * this contract accepts no table name, predicate, SQL, or query from a caller. */
export function financeOwnedSnapshotSources(){return {
  "finance_procurement_bill_matches_from": `FROM (${projections.procurement_bill_matches}) procurement_bill_matches`,
  "supplier_bill_draft_void_procurement_bill_match_receipts_br_from": `FROM (${projections.procurement_bill_match_receipts}) br`
} as const;}
