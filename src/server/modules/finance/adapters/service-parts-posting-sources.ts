import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {WorkContext} from '../../identity_access/public/authorization';
import {commercePostingFinanceReferences} from './commerce-posting-setup';
import type {ServicePartsPostingSetupInput} from '../../commerce/public/service-parts-posting-setup';
import {ApiError} from '../../runtime/public/http';
import {approvedSalesPostingDefaults,approvedSalesPostingDefaultMatches,approvedSalesPostingDefaultSourceLocks} from './approved-sales-posting-defaults';
/** Exact Finance-owned reference/scale witness for Service mapping context and
 * finality. The native validator remains the original nine-account validator. */
export async function servicePartsPostingFinanceReferences(database:SqlDatabase,actor:WorkContext){
 const references=await commercePostingFinanceReferences(database,actor),salesDefaults=await approvedSalesPostingDefaults(database,actor.companyId),money=await database.prepare(`SELECT jsonb_build_object('settings',(SELECT to_jsonb(settings) FROM company_money_settings settings WHERE settings.company_id=?),'scales',COALESCE((SELECT jsonb_agg(to_jsonb(scale) ORDER BY scale.currency) FROM company_currency_scales scale WHERE scale.company_id=?),'[]'::jsonb))::text image`).bind(actor.companyId,actor.companyId).first<string>('image');
 return {...references,salesDefaults,image:JSON.stringify([references.image,money,salesDefaults.image])};
}

export const servicePartsPostingFinanceDefaultSourceLocks=approvedSalesPostingDefaultSourceLocks;
/** Fresh Service setup may add genuine posting roles without replacing the
 * independently reviewed Sales intent, even before the first Sales document. */
export async function validateServicePartsPostingSalesDefaults(database:SqlDatabase,actor:WorkContext,input:ServicePartsPostingSetupInput){
 const state=await approvedSalesPostingDefaults(database,actor.companyId);
 if(state.defaultsBlocked)throw new ApiError(409,'SALES_DEFAULTS_REQUIRED','Restore the independently approved Sales defaults before initial Service parts posting setup.');
 if(JSON.stringify(input.expectedSalesDefaults??null)!==JSON.stringify(state.expectedSalesDefaults)||!approvedSalesPostingDefaultMatches(state.defaults,input))throw new ApiError(409,'SERVICE_PARTS_POSTING_DEFAULTS_CHANGED','Retain the current independently approved Sales ledger, currency, receivable, revenue and VAT settings.');
}
