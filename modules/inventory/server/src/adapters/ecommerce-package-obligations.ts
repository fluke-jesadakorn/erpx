import 'server-only';
import {createHash} from 'node:crypto';
import type {PostgresDatabase} from '@erpx/platform-runtime-server/public/runtime/postgres';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import type {PackageSettlementSourceLock} from '@erpx/commercial-server/public/package-settlement-sources';
import {assertEcommercePackageObligationSources} from '@erpx/ecommerce-server/public/package-obligations';
import {ecommerceQualitySettlementSources} from '@erpx/quality-server/public/ecommerce-package-sources';
import {lockSalesInventoryPackageObligationSources,assertSalesInventoryPackageObligationSources,readSalesInventorySettlementFacts,salesInventorySettlementSources} from '@erpx/inventory-server/adapters/sales-package-obligations';
import {disposalCountsSql} from './read-tail-sql';
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};
export async function lockEcommerceInventoryPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){await lockSalesInventoryPackageObligationSources(owner,lock);}
export const ecommerceInventorySettlementSources={...salesInventorySettlementSources,disposalCounts:disposalCountsSql(ecommerceQualitySettlementSources.returnHoldSources)} as const;
export async function ecommerceInventoryPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 assertEcommercePackageObligationSources(owner,database,input.companyId);assertSalesInventoryPackageObligationSources(owner,database,input.companyId);if(input.packageId!=='sales-ecommerce'||input.packageVersion!==1||input.moduleId!=='inventory'||!Number.isSafeInteger(input.now)||input.now<0)throw Error('ECOMMERCE_INVENTORY_PACKAGE_OBLIGATION_SCOPE_REQUIRED');
 const facts=await readSalesInventorySettlementFacts(database,input.companyId,'ecommerce');return {companyId:input.companyId,packageId:input.packageId,moduleId:'inventory',observedAt:input.now,revision:createHash('sha256').update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts})).digest('hex'),outstanding:facts.filter(row=>row.pending!==false).length};
}
