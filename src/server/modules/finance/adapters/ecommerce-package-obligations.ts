import 'server-only';
import {createHash} from 'node:crypto';
import type {PostgresDatabase} from '../../runtime/public/runtime/postgres';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {packageSettlementSource,type PackageSettlementSourceLock} from '../../commercial/public/package-settlement-sources';
import {assertEcommercePackageObligationSources} from '../../ecommerce/public/package-obligations';
import {assertSalesInventoryPackageObligationSources} from '../../inventory/public/sales-package-obligations';
import {assertSalesPackageObligationSources} from '../../sales/public/package-obligations';
import {lockSalesFinancePackageObligationSources,readSalesFinanceSettlementFacts} from './sales-package-obligations';
const sources=new WeakMap<SqlDatabase,string>();
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};
export async function lockEcommerceFinancePackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){const {database,companyId}=packageSettlementSource(owner,lock);await lockSalesFinancePackageObligationSources(owner,lock);sources.set(database,companyId);}
/** Actual original included Sales-owned money only: primary and reviewed
 * secondary journals, allocations, returns, physical disposal and refunds. */
export async function ecommerceFinancePackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 owner.assertActiveTransaction(database);if(sources.get(database)!==input.companyId)throw Error('ECOMMERCE_FINANCE_PACKAGE_OBLIGATION_SOURCE_REQUIRED');assertEcommercePackageObligationSources(owner,database,input.companyId);assertSalesPackageObligationSources(owner,database,input.companyId);assertSalesInventoryPackageObligationSources(owner,database,input.companyId);if(input.packageId!=='sales-ecommerce'||input.packageVersion!==1||input.moduleId!=='finance'||!Number.isSafeInteger(input.now)||input.now<0)throw Error('ECOMMERCE_FINANCE_PACKAGE_OBLIGATION_SCOPE_REQUIRED');
 const facts=await readSalesFinanceSettlementFacts(database,input.companyId,'ecommerce');return {companyId:input.companyId,packageId:input.packageId,moduleId:'finance',observedAt:input.now,revision:createHash('sha256').update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts})).digest('hex'),outstanding:facts.filter(row=>row.pending!==false).length};
}
