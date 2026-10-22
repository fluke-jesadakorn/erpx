import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Original Company placement identity, including retained inactive nodes. */
export const talentSettlementPlacements="SELECT id,company_id,branch_id,kind,status FROM organization_nodes";
export function talentSettlementPlacementReadLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE organization_nodes IN SHARE MODE")];}
