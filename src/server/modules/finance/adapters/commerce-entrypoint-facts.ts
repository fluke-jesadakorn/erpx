import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {commerceEntrypointReadSources} from '../../commerce/public/entrypoint-facts';
export function commerceEntrypointFinanceFacts(db:SqlDatabase){return {
 partyKind:(id:string,companyId:string)=>db.prepare('SELECT kind FROM finance_parties WHERE id=? AND company_id=?').bind(id,companyId).first<{kind:string}>(),
 editedPartyKind:(id:string,companyId:string)=>db.prepare('SELECT kind FROM finance_parties WHERE id=? AND company_id=? AND person_id IS NULL').bind(id,companyId).first<{kind:string}>(),
 fxAccounts:(companyId:string)=>db.prepare(`SELECT a.id,a.code,a.name,a.category FROM finance_accounts a
      JOIN finance_charts c ON c.id=a.chart_id AND c.company_id=a.company_id JOIN (${commerceEntrypointReadSources.settings}) s ON s.chart_id=c.id AND s.company_id=c.company_id
      WHERE a.company_id=? AND a.currency=c.base_currency AND a.category IN ('other_income','other_expense') AND a.status='active' ORDER BY a.code`).bind(companyId).all<Record<string,string|number|null>>()
};}
