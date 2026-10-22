import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Original immutable salary capture, never a rerate or current Payroll read. */
export const talentHireSettlementMoney="SELECT company_id,source_id,transaction_date,currency,currency_scale,amount_units FROM money_transaction_snapshots WHERE source_type='talent_hire_compensation' AND amount_kind='base'";
export function talentHireSettlementMoneyReadLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE money_transaction_snapshots IN SHARE MODE")];}
