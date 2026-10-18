import "server-only";
import {hospitalityOwnerProjectionSources as commerce,hospitalitySettlementReferenceReadLocks} from "../../commerce/public/hospitality-owner-projection-sources";
import { createHash } from "node:crypto";
import type { PostgresDatabase } from "../../runtime/public/runtime/postgres";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import { packageSettlementSource,type PackageSettlementSourceLock } from "../../commercial/public/package-settlement-sources";
import { assertPosAddonPackageObligationSources,posAddonPackageSettlementSources } from "../../pos/public/addon-package-obligations";
import { assertPmsAddonPackageObligationSources,pmsAddonPackageSettlementSources } from "../../pms/public/addon-package-obligations";

type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};
const sources=new WeakMap<SqlDatabase,string>();
export async function lockHospitalityAddonInventoryPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){
  const {database,companyId}=packageSettlementSource(owner,lock);
  await database.batch(hospitalitySettlementReferenceReadLocks(database));
  await database.prepare('LOCK TABLE commerce_stock IN SHARE MODE').run();
  sources.set(database,companyId);
}
export async function hospitalityAddonInventoryPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
  owner.assertActiveTransaction(database);
  if(sources.get(database)!==input.companyId)throw new Error('HOSPITALITY_ADDON_INVENTORY_SOURCE_REQUIRED');
  if(input.moduleId!=='inventory'||!['pos-restaurant','pos-hotel-charge','pms-minibar-linen'].includes(input.packageId)||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw new Error('HOSPITALITY_ADDON_INVENTORY_SCOPE_REQUIRED');
  const posSource=input.packageId!=='pms-minibar-linen',pmsSource=input.packageId==='pms-minibar-linen';
  if(posSource)assertPosAddonPackageObligationSources(owner,database,input.companyId,input.packageId);
  if(pmsSource)assertPmsAddonPackageObligationSources(owner,database,input.companyId,input.packageId);
  const pos=posSource?posAddonPackageSettlementSources(input.packageId):null;
  const pms=pmsSource?pmsAddonPackageSettlementSources(input.packageId):null;
  const posFacts=pos?(await database.prepare(`WITH scope AS (SELECT ?::text company_id,?::text package_id)
    SELECT 'reservation' kind,concat(r.fulfillment_id,':',r.warehouse_id,':',r.product_id) id,r.version,r.quantity_milli amount,true pending FROM (${pos.reservations}) r,scope WHERE r.company_id=scope.company_id AND r.package_id=scope.package_id
    UNION ALL SELECT 'movement',m.id,COALESCE(s.version,0),m.value_minor,s.warehouse_id IS NULL OR w.id IS NULL OR p.id IS NULL OR s.quantity_milli<0 OR s.value_minor<0 OR (m.kind IN ('sale','kitchen') AND (m.quantity_milli>=0 OR m.value_minor>0)) OR (m.kind='return' AND (m.quantity_milli<=0 OR m.value_minor<0))
      FROM (${pos.stockMoves}) m LEFT JOIN commerce_stock s ON s.warehouse_id=m.warehouse_id AND s.product_id=m.product_id LEFT JOIN (${commerce.commerce_warehouses}) w ON w.id=m.warehouse_id AND w.company_id=m.company_id LEFT JOIN (${commerce.commerce_products}) p ON p.id=m.product_id AND p.company_id=m.company_id,scope WHERE m.company_id=scope.company_id AND m.package_id=scope.package_id
    UNION ALL SELECT 'consumption',f.id,f.version,f.cost_minor,true FROM (${pos.fulfillments}) f,scope WHERE f.company_id=scope.company_id AND f.package_id=scope.package_id AND (
      f.kind='stock' AND f.state='delivered' OR f.kind='recipe' AND f.state IN ('preparing','ready','delivered'))
      AND NOT EXISTS(SELECT 1 FROM (${pos.stockMoves}) m WHERE m.company_id=f.company_id AND m.package_id=f.package_id AND m.fulfillment_id=f.id AND m.kind IN ('sale','kitchen'))
    ORDER BY kind,id`).bind(input.companyId,input.packageId).all()).results:[];
  const pmsFacts=pms?(await database.prepare(`SELECT 'movement' kind,m.id,m.version,COALESCE(m.value_minor,0) amount,
      m.actual_warehouse_id IS NULL OR m.warehouse_id IS DISTINCT FROM m.actual_warehouse_id OR m.product_id IS DISTINCT FROM m.actual_product_id OR m.quantity_milli IS DISTINCT FROM m.expected_quantity OR m.value_minor>0 OR s.warehouse_id IS NULL OR w.id IS NULL OR p.id IS NULL OR s.quantity_milli<0 OR s.value_minor<0 pending
    FROM (${pms.stockMoves}) m LEFT JOIN commerce_stock s ON s.warehouse_id=m.warehouse_id AND s.product_id=m.product_id LEFT JOIN (${commerce.commerce_warehouses}) w ON w.id=m.warehouse_id AND w.company_id=m.company_id LEFT JOIN (${commerce.commerce_products}) p ON p.id=m.product_id AND p.company_id=m.company_id WHERE m.company_id=? AND m.package_id=? ORDER BY kind,id`).bind(input.companyId,input.packageId).all()).results:[];
  const facts=[...posFacts,...pmsFacts];
  return {companyId:input.companyId,packageId:input.packageId,moduleId:'inventory',observedAt:input.now,revision:createHash('sha256').update(JSON.stringify({companyId:input.companyId,packageId:input.packageId,packageVersion:input.packageVersion,facts})).digest('hex'),outstanding:facts.filter(fact=>fact.pending!==false).length};
}
