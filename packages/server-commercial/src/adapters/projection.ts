import {commercialSeatProjectionAccounts} from '@erpx/server-identity_access/public/commercial-seat-facts';
import {coverageProbe} from './commercial-read-tail-r4-sources';
import {holdingSnapshotReadSql} from './commercial-read-tail-r9-sources';
import {readNativeProjectionFacts,type Lane37Actor,type ProjectionFacts} from './native-lane-37/client';
import {ApiError} from '@erpx/server-runtime/public/http';

import { nativeCommercialOwnerWrite, NativeCommercialOwnerWriteError } from "@erpx/server-commercial/adapters/native-owner-write";
async function ownerWrite(kind: any, values: readonly unknown[], recordId: string, key: string) {
  try { return await nativeCommercialOwnerWrite(kind, values, recordId, 'system', key); }
  catch (error) { if (error instanceof NativeCommercialOwnerWriteError) throw new Error(error.code + ': ' + error.message); throw error; }
}
import {commercialEarliestOriginal} from '@erpx/server-files/public/commercial-usage-facts';
import {commercialEarliestAIUsage} from '@erpx/server-agent/public/commercial-usage-facts';
import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {createUsageProjectionApplication,type UsageProjectionRepository,type UsageProjectionTransaction} from '@erpx/server-commercial/application/projection';
import type {Binding,Snapshot,SnapshotHeader,MeterRow} from '@erpx/server-commercial/domain/projection';
import {readHoldingUsage} from '@erpx/server-commercial/adapters/usage';
import {readControlSeatSummary} from '@erpx/server-commercial/adapters/seats';
function transaction(tx:SqlDatabase):UsageProjectionTransaction{return {
 principalCoverage:()=>tx.prepare(coverageProbe("since_at","billing_principal_status_coverage","TRUE"))
    .first<{since_at:number}>(),principalAccounts:async ids=>{return (await commercialSeatProjectionAccounts(tx,ids)).results;},principalEvents:async(end,ids)=>{const billablePlaceholders=ids.map(()=>'?').join(','),billableIds=ids;return (await tx.prepare(`SELECT id,user_id,status,effective_at
    FROM billing_principal_status_events WHERE effective_at<? AND user_id IN (${billablePlaceholders})
    ORDER BY user_id,effective_at,id`)
    .bind(end,...billableIds)
    .all<{id:number;user_id:string;status:string;effective_at:number}>()).results;},
 async ensureHeader(plan){const {binding,month,snapshotId,observedAt,coverage,activeSeat}=plan;
   await ownerWrite('usage.snapshot-ensure', [binding.id, month, snapshotId, observedAt, coverage.ai, coverage.storage,
     coverage.unreconciledOriginals,activeSeat?.count??null,activeSeat?.asOf??null], binding.id, `usage:${snapshotId}:ensure`);},lockHeader:(holdingId,month)=>{const binding={id:holdingId};return tx.prepare(holdingSnapshotReadSql("refreshed_at", " AND month=? FOR UPDATE")).bind(binding.id, month).first<{refreshed_at:number}>();},async meter(plan,row){const {snapshotId}=plan;
   await ownerWrite('usage.meter', [row.holding_id,row.month,row.source_key,row.company_id,row.company_name,
     row.branch_id,row.branch_name,row.meter_kind,row.funding_source,row.pricing_status,
     row.quantity,row.input_tokens,row.output_tokens,row.average_daily_bytes,
     row.active_at_period_end,snapshotId,row.request_status,row.gateway_status,row.provider,
     row.model,row.units_json,row.cost_status,row.verified_cost_satang,
     row.cost_evidence_key,row.cost_reconciled_at,row.active_from,row.active_until,
     row.byte_milliseconds], row.holding_id, `usage:${snapshotId}:meter:${row.source_key}`);},async pruneMeters(plan){const {binding,month,snapshotId}=plan;
   await ownerWrite('usage.meter-prune', [binding.id, month, snapshotId], binding.id, `usage:${snapshotId}:meter-prune`);},async seat(plan,seat){const {binding,month,snapshotId}=plan;
   await ownerWrite('usage.seat', [binding.id,month,seat.userKey,JSON.stringify(seat.intervals),snapshotId], binding.id, `usage:${snapshotId}:seat:${seat.userKey}`);},async pruneSeats(plan){const {binding,month,snapshotId}=plan;
   await ownerWrite('usage.seat-prune', [binding.id,month,snapshotId], binding.id, `usage:${snapshotId}:seat-prune`);},async finish(plan,sourceCount,seatProjection){const {binding,month,snapshotId,observedAt,coverage,activeSeat}=plan,meters={size:sourceCount};
   await ownerWrite('usage.snapshot-finish', [snapshotId, observedAt, meters.size, coverage.ai, coverage.storage,
     seatProjection.coverage,seatProjection.rows.length, coverage.unreconciledOriginals,activeSeat?.count??null,
     activeSeat?.asOf??null,binding.id,month], binding.id, `usage:${snapshotId}:finish`);},
};}
/** Map the owner's camelCase facts to the snake_case row shapes the application
 * consumes. The native client already validated every field; no value is
 * defaulted, coerced or synthesised here. */
type ProjectionRepositoryFacts={snapshots:SnapshotHeader[];sourceRows:MeterRow[];currentSnapshots:Pick<SnapshotHeader,'holding_id'|'active_seat_count'|'active_seat_observed_at'>[]};
export function nativeFactsToRepository(facts:ProjectionFacts,current:ProjectionFacts):ProjectionRepositoryFacts {
 const snapshots:SnapshotHeader[]=facts.snapshots.map(row=>({
   holding_id:row.holdingId,ai_coverage:row.aiCoverage as SnapshotHeader['ai_coverage'],
   storage_coverage:row.storageCoverage as SnapshotHeader['storage_coverage'],
   seat_coverage:row.seatCoverage as SnapshotHeader['seat_coverage'],
   seat_source_count:row.seatSourceCount,unreconciled_originals:row.unreconciledOriginals,
   active_seat_count:row.activeSeatCount,active_seat_observed_at:row.activeSeatObservedAt,
 }));
 const sourceRows:MeterRow[]=facts.sourceRows.map(row=>({
   holding_id:row.holdingId,month:row.month,source_key:row.sourceKey,
   company_id:row.companyId,company_name:row.companyName,
   branch_id:row.branchId,branch_name:row.branchName,
   meter_kind:row.meterKind as MeterRow['meter_kind'],
   funding_source:row.fundingSource as MeterRow['funding_source'],
   pricing_status:row.pricingStatus as MeterRow['pricing_status'],
   quantity:row.quantity,input_tokens:row.inputTokens,output_tokens:row.outputTokens,
   average_daily_bytes:row.averageDailyBytes,active_at_period_end:row.activeAtPeriodEnd,
   request_status:row.requestStatus as MeterRow['request_status'],
   gateway_status:row.gatewayStatus as MeterRow['gateway_status'],
   provider:row.provider,model:row.model,units_json:row.unitsJson,
   cost_status:row.costStatus as MeterRow['cost_status'],
   verified_cost_satang:row.verifiedCostSatang,cost_evidence_key:row.costEvidenceKey,
   cost_reconciled_at:row.costReconciledAt,active_from:row.activeFrom,active_until:row.activeUntil,
   byte_milliseconds:row.byteMilliseconds,
 }));
 const currentSnapshots=current.currentSnapshots.map(row=>({
   holding_id:row.holdingId,active_seat_count:row.activeSeatCount,active_seat_observed_at:row.activeSeatObservedAt,
 }));
 return {snapshots,sourceRows,currentSnapshots};
}
function repository(control:SqlDatabase,actor?:Lane37Actor):UsageProjectionRepository{return {async projectionFacts(month,currentMonth){
  if(!actor)throw new ApiError(503,'LANE37_ACTOR_REQUIRED','The native Commercial projection read requires the current Holding.');
  const [facts,current]=await Promise.all([readNativeProjectionFacts(actor,month),readNativeProjectionFacts(actor,currentMonth)]);
  return nativeFactsToRepository(facts,current);},seatSummary:(holdingId,month)=>readControlSeatSummary(control,holdingId,month),async history(binding){const source = (await holdingResources(binding.id)).write;
      const [ai, storage, existing] = await Promise.all([
        commercialEarliestAIUsage(source,),
        commercialEarliestOriginal(source,{holdingId:binding.id,holdingSlug:binding.slug}),
        control.prepare(holdingSnapshotReadSql("month", ""))
          .bind(binding.id).all<{ month: string }>(),
      ]);
      return {ai,storage,existing:existing.results};}};}
function application(control:SqlDatabase,actor?:Lane37Actor){return createUsageProjectionApplication({repository:repository(control,actor),transactions:isTransactionalSqlDatabase(control)?{run:action=>control.transaction(tx=>action(transaction(tx)))}:undefined,clock:{now:()=>Date.now()},identifiers:{next:randomUUID},hash:value=>createHash('sha256').update(value).digest('hex'),readHolding:readHoldingUsage});}
export function syncHoldingUsageProjection(control:SqlDatabase,binding:Binding,month:string,snapshot:Snapshot,observedAt=Date.now()){return application(control).syncHoldingUsageProjection(binding,month,snapshot,observedAt);}
export function readPlatformUsageProjection(control:SqlDatabase,bindings:Binding[],month:string,selectedId:string|null=null){
  const selected=selectedId?bindings.filter(binding=>binding.id===selectedId):bindings,first=selected[0];
  const actor=first?{holdingId:first.id,holdingSlug:first.slug}:undefined;
  return application(control,actor).readPlatformUsageProjection(bindings,month,selectedId);}
export function refreshRecentUsageProjections(control:SqlDatabase,bindings:Binding[],now=Date.now()){return application(control).refreshRecentUsageProjections(bindings,now);}
export function bootstrapUsageProjectionHistory(control:SqlDatabase,bindings:Binding[],now=Date.now(),lookbackMonths=24){return application(control).bootstrapUsageProjectionHistory(bindings,now,lookbackMonths);}




