import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {commerceFinanceJoinedFacts} from './commerce-joined-facts';
export function salesEditorFinanceSourceReadLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE finance_accounts,finance_charts,finance_parties,finance_sales_defaults IN SHARE MODE')];}

/** The Sales editor needs customer labels and its original posting currency.
 * Supplier, balance, bank and other Finance reference images stay on the server. */
export async function salesEditorFinanceReferences(database:SqlDatabase,companyId:string){
 const configuration=await commerceFinanceJoinedFacts(database).readCommerceSettings1({companyId});
 if(!configuration)return null;
 const currency=String(configuration.currency),customers=(await database.prepare("SELECT id,name,kind,status,currency FROM finance_parties WHERE company_id=? AND kind='customer' AND person_id IS NULL AND status='active' AND currency=? ORDER BY name").bind(companyId,currency).all<{id:string;name:string;kind:string;status:string;currency:string}>()).results;
 return {currency,customers};
}
