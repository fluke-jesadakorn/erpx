import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";

export type ServiceCustomerReference = { id: string; name: string; currency: string };

/** Closed customer identity used only by the original Service owner. */
export const fieldServiceCustomerSourceRows = "SELECT id,company_id,kind,status,currency FROM finance_parties";
export function fieldServiceFinanceSourceReadLocks(database:SqlDatabase) {
  return [database.prepare("LOCK TABLE finance_parties,finance_charts,finance_accounts,finance_periods,finance_book_rules,company_money_settings,company_currency_scales,company_fx_rates IN SHARE MODE")];
}
export function fieldServiceCustomerAttachmentSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE finance_parties IN SHARE ROW EXCLUSIVE MODE');}
export function fieldServiceCustomerAttachmentSourceImage(database:SqlDatabase,companyId:string,id:string){return database.prepare('SELECT to_jsonb(source)::text image FROM finance_parties source WHERE source.id=? AND source.company_id=?').bind(id,companyId).first<string>('image');}

/** Minimal customer/currency foundation for a Service agreement, without Finance management access. */
export async function fieldServiceCustomers(database: SqlDatabase, companyId: string): Promise<ServiceCustomerReference[]> {
  return (await database.prepare("SELECT id,name,currency FROM finance_parties WHERE company_id=? AND kind='customer' AND status='active' ORDER BY name,id")
    .bind(companyId).all<ServiceCustomerReference>()).results;
}
