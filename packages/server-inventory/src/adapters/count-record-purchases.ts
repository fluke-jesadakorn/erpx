import {serviceInitialPhysicalStockCapabilities,serviceInitialPhysicalStockCountSourceRows,serviceInitialPhysicalStockMarkedRows} from '@erpx/server-inventory/adapters/service-initial-physical-stock-count-source-sql';
import {posInitialPhysicalStockCapabilities,posInitialPhysicalStockCountSourceRows,posInitialPhysicalStockMarkedRows} from '@erpx/server-inventory/adapters/pos-initial-physical-stock-count-source-sql';
import {manufacturingInitialPhysicalStockCountLanding} from '@erpx/server-inventory/domain/manufacturing-initial-physical-stock-entry';
import {manufacturingInitialPhysicalStockCapabilities,manufacturingInitialPhysicalStockCountSourceRows,manufacturingInitialPhysicalStockMarkedRows} from '@erpx/server-inventory/adapters/manufacturing-initial-physical-stock-count-source-sql';
import "server-only";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {commercePackageEnforcementMode,readPurchasedAccess} from "@erpx/server-commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "@erpx/server-commercial/public/purchased-capability-sql";
import {initialPhysicalStockCountSourceRows,initialPhysicalStockMarkedRows} from '@erpx/server-inventory/adapters/initial-physical-stock-count-source-sql';
import {requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {ApiError} from '@erpx/server-runtime/public/http';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {qualityDisposalCountRows} from "@erpx/server-inventory/adapters/quality-count-purchases";
import {requireCountPurchasedRead} from "@erpx/server-inventory/adapters/count-purchases";
import {allInitialPhysicalStockMarkedRowsSql,countInitialMarkedExistsSql,sourceRowsExistsSql,nativeRecordExistsSql,branchScopeTupleSql,recordIdExistsSql} from './read-tail-sql';

/** Filter actual physical-count identities before the consumer limit. Other
 * Inventory records, shared Stock facts and durable deliveries retain owners. */
export function countSourcePurchasePredicate(id:string,company:string,purpose:"history"|"settle-existing",clock:string){
 const marked=countInitialMarkedExistsSql(initialPhysicalStockMarkedRows,id),manufacturingMarked=countInitialMarkedExistsSql(manufacturingInitialPhysicalStockMarkedRows,id,'marker');
 const initial=sourceRowsExistsSql(initialPhysicalStockCountSourceRows,'initial_source',id,company,`${['sales.orders','inventory.stock-core','finance.sales-posting'].map(capability=>purchasedCapabilityPredicate('initial_source.company_id',capability,purpose,clock)).join(' AND ')} AND ${['sales','inventory','finance'].map(module=>scheduledFeaturePredicate('initial_source.company_id',module)).join(' AND ')}`);
 const ordinary=`(${purchasedCapabilityPredicate(company,"supply.counts",purpose,clock)} OR EXISTS(SELECT 1 FROM (${qualityDisposalCountRows}) actual_disposal WHERE actual_disposal.id=${id} AND actual_disposal.company_id=${company} AND actual_disposal.kind='customer-return' AND ${purchasedCapabilityPredicate("actual_disposal.company_id","sales.orders",purpose,clock)} AND ${purchasedCapabilityPredicate("actual_disposal.company_id","inventory.stock-core",purpose,clock)}))`;
 const manufacturing=sourceRowsExistsSql(manufacturingInitialPhysicalStockCountSourceRows,'manufacturing_source',id,company,`${manufacturingInitialPhysicalStockCapabilities.map(capability=>purchasedCapabilityPredicate('manufacturing_source.company_id',capability,purpose,clock)).join(' AND ')} AND ${['manufacturing','inventory','finance'].map(module=>scheduledFeaturePredicate('manufacturing_source.company_id',module)).join(' AND ')}`);
 const serviceMarked=countInitialMarkedExistsSql(serviceInitialPhysicalStockMarkedRows,id,'marker'),service=sourceRowsExistsSql(serviceInitialPhysicalStockCountSourceRows,'pos_source',id,company,`${serviceInitialPhysicalStockCapabilities.map(cap=>purchasedCapabilityPredicate('pos_source.company_id',cap,purpose,clock)).join(' AND ')} AND ${['field_service','inventory','finance'].map(module=>scheduledFeaturePredicate('pos_source.company_id',module)).join(' AND ')}`);
 const posMarked=countInitialMarkedExistsSql(posInitialPhysicalStockMarkedRows,id,'marker'),pos=sourceRowsExistsSql(posInitialPhysicalStockCountSourceRows,'pos_source',id,company,`${posInitialPhysicalStockCapabilities.map(cap=>purchasedCapabilityPredicate('pos_source.company_id',cap,purpose,clock)).join(' AND ')} AND ${['pos','inventory','finance'].map(module=>scheduledFeaturePredicate('pos_source.company_id',module)).join(' AND ')}`);
 return `(CASE WHEN ${serviceMarked} THEN ${service} WHEN ${posMarked} THEN ${pos} WHEN ${manufacturingMarked} THEN ${manufacturing} WHEN ${marked} THEN ${initial} ELSE ${ordinary} END)`;
}
export async function countSourceReadScope(database:SqlDatabase,actor:WorkContext,surface:"staff"|"inbox"|"records"){
 const mode=await commercePackageEnforcementMode();
 if(mode==='shadow')await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'supply.counts'});
 const id=surface==="staff"?"staff_notifications.record_id":surface==="inbox"?"e.source_id":"workspace_records.id";
 const company=surface==="staff"?"staff_notifications.company_id":surface==="inbox"?"e.company_id":"workspace_records.company_id";
 const relevant=surface==="records"?`(module_id='inventory' AND ${nativeRecordExistsSql('inventory_counts','native_count',id)})`:`${surface==="staff"?"staff_notifications.record_type":"e.source_type"} IN ('stock_count','inventory_stock_count')`;
 const initialMarked=`EXISTS(${allInitialPhysicalStockMarkedRowsSql(initialPhysicalStockMarkedRows,manufacturingInitialPhysicalStockMarkedRows,posInitialPhysicalStockMarkedRows,serviceInitialPhysicalStockMarkedRows)} initial_marker WHERE initial_marker.id=${id})`;
 if(actor.departmentId||!hasPermission(actor,'inventory.read'))return {where:mode==='enforced'?`NOT (${relevant})`:`(NOT (${relevant}) OR NOT (${initialMarked}))`,args:[]};
 const predicate=countSourcePurchasePredicate('count_source.id','count_source.company_id','history','count_source_clock.now_ms');
 const allowed=mode==='enforced'?predicate:`(NOT EXISTS(${allInitialPhysicalStockMarkedRowsSql(initialPhysicalStockMarkedRows,manufacturingInitialPhysicalStockMarkedRows,posInitialPhysicalStockMarkedRows,serviceInitialPhysicalStockMarkedRows)} marker WHERE marker.id=count_source.id) OR ${predicate})`;
 const visible=`EXISTS(WITH count_source_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) SELECT 1 FROM (SELECT id,company_id,branch_id,NULL::text department_id FROM inventory_counts) count_source CROSS JOIN count_source_clock WHERE count_source.id=${id} AND count_source.company_id=${company} AND ${branchScopeTupleSql('count_source')} AND ${assignmentReadSql("count_source","inventory.read")} AND ${allowed})`;
 return {where:mode==='enforced'?`(NOT (${relevant}) OR ${visible})`:`(NOT (${relevant}) OR NOT (${initialMarked}) OR ${visible})`,args:[actor.branchId,actor.branchId,actor.assignmentId,actor.userId,actor.role]};
}
export async function requireCountWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,moduleId:string,id:string){
 if(moduleId!=="inventory")return;
 if(await database.prepare(recordIdExistsSql("inventory_counts")).bind(id).first())await requireCountPurchasedRead(database,actor,id);
}

/** Resolve only native validated owner destinations after the shared reader's
 * before-limit visibility filter; callers never supply an arbitrary href. */
export async function initialPhysicalStockCountDestinations(database:SqlDatabase,actor:WorkContext,ids:readonly string[]){
 const selected=[...new Set(ids)].slice(0,500);if(!selected.length)return {} as Record<string,string>;
 if(actor.departmentId||!hasPermission(actor,'inventory.read'))return {};
 try{await requireQueuedCommandAuthority(database,actor,'inventory.read');}catch(error){if(error instanceof ApiError&&(error.status===401||error.status===403))return {};throw error;}
 const rows=await database.prepare(`WITH initial_destination_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) SELECT source.id,source.kind FROM (SELECT sales.*, 'sales'::text kind FROM (${initialPhysicalStockCountSourceRows}) sales UNION ALL SELECT manufacturing.*, 'manufacturing'::text kind FROM (${manufacturingInitialPhysicalStockCountSourceRows}) manufacturing UNION ALL SELECT pos.*, 'pos'::text kind FROM (${posInitialPhysicalStockCountSourceRows}) pos UNION ALL SELECT service.*, 'service'::text kind FROM (${serviceInitialPhysicalStockCountSourceRows}) service) source CROSS JOIN initial_destination_clock WHERE source.company_id=? AND source.id IN (${selected.map(()=>'?').join(',')}) AND ${branchScopeTupleSql('source')} AND ${countSourcePurchasePredicate('source.id','source.company_id','history','initial_destination_clock.now_ms')}`).bind(actor.companyId,...selected,actor.branchId,actor.branchId).all<{id:string;kind:'sales'|'manufacturing'|'pos'|'service'}>();
 return Object.fromEntries(rows.results.map(row=>[row.id,row.kind==='service'?`/services/field-service/settings/initial-stock/${encodeURIComponent(row.id)}`:row.kind==='pos'?`/pos/settings/initial-stock/${encodeURIComponent(row.id)}`:row.kind==='manufacturing'?`/${manufacturingInitialPhysicalStockCountLanding}/${encodeURIComponent(row.id)}`:`/customers-sales/sales/settings/initial-stock/${encodeURIComponent(row.id)}`]));
}
