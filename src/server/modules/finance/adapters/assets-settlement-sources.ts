import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export const assetsSettlementFinancialAssets="SELECT id,company_id,status,currency,cost_minor,residual_minor,accumulated_depreciation_minor,activation_journal_id,version FROM finance_assets";
export function assetsSettlementFinancialAssetReadLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE finance_assets IN SHARE MODE")];}
