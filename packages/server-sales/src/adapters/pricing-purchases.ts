import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {assignmentReadSql} from '@erpx/server-identity_access/public/record-source-authority';
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import {commercePackageEnforcementMode,readPurchasedAccesses} from '@erpx/server-commercial/public/purchase-command-boundary';
import {crmFinanceSourceReadLocks} from '@erpx/server-finance/public/crm-purchase-sources';
import {salesPricingPurchaseSourceReadLocks} from '@erpx/server-commerce/public/sales-order-sources';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
type Controls={newWork:boolean;settleExisting:boolean;history:boolean;revision:string};
function companyScope(actor:WorkContext){if(actor.branchId||actor.departmentId)throw new ApiError(403,'COMPANY_PRICING_SCOPE_REQUIRED','Use a Company sales assignment to manage price lists.');}
async function authority(database:SqlDatabase,actor:WorkContext){companyScope(actor);for(const permission of ['commerce.read','commerce.sales']){if(!hasPermission(actor,permission)||!await database.prepare(`SELECT ${assignmentReadSql('pricing_scope',permission)} allowed FROM (SELECT ?::text company_id,NULL::text branch_id,NULL::text department_id) pricing_scope`).bind(actor.assignmentId,actor.userId,actor.role,actor.companyId).first<boolean>('allowed'))throw new ApiError(403,'SALES_PRICING_ACCESS_CHANGED','The original Company pricing assignment is no longer current.');}}
/** Retained read controls and source leases. Pricing commands are native-owned. */
export function salesPricingPurchaseReadLocks(database:SqlDatabase){return [...crmFinanceSourceReadLocks(database),...salesPricingPurchaseSourceReadLocks(database),database.prepare('LOCK TABLE sales_price_lists,sales_price_list_items IN SHARE ROW EXCLUSIVE MODE')];}
export async function salesPricingPurchaseControls(database:SqlDatabase,actor:WorkContext):Promise<Controls|undefined>{const mode=await commercePackageEnforcementMode();if(mode==='disabled')return undefined;await authority(database,actor);const value=(await readPurchasedAccesses(database,mode,actor.companyId,['sales.orders']))['sales.orders'];if(mode==='enforced'&&!value.history)throw new ApiError(403,'PACKAGE_REQUIRED','Sales retained pricing history is required.');return {newWork:value.newWork,settleExisting:value.settleExisting,history:value.history,revision:value.revision};}
