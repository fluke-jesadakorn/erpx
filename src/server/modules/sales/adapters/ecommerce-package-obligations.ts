import 'server-only';
import {createHash} from 'node:crypto';
import type {PostgresDatabase} from '../../runtime/public/runtime/postgres';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {PackageSettlementSourceLock} from '../../commercial/public/package-settlement-sources';
import {assertEcommercePackageObligationSources} from '../../ecommerce/public/package-obligations';
import {lockSalesPackageObligationSources,assertSalesPackageObligationSources,readSalesPackageSettlementFacts} from './package-obligations';
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};
export async function lockEcommerceSalesPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){await lockSalesPackageObligationSources(owner,lock);}
export async function ecommerceSalesPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 assertEcommercePackageObligationSources(owner,database,input.companyId);assertSalesPackageObligationSources(owner,database,input.companyId);if(input.packageId!=='sales-ecommerce'||input.packageVersion!==1||input.moduleId!=='sales'||!Number.isSafeInteger(input.now)||input.now<0)throw Error('ECOMMERCE_SALES_PACKAGE_OBLIGATION_SCOPE_REQUIRED');
 const facts=await readSalesPackageSettlementFacts(database,input.companyId,'ecommerce');return {companyId:input.companyId,packageId:input.packageId,moduleId:'sales',observedAt:input.now,revision:createHash('sha256').update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts})).digest('hex'),outstanding:facts.filter(row=>row.pending!==false).length};
}
