import {commercialSeatCutoffDependency} from '@erpx/people-payroll-server/public/commercial-seat-cutoff';
import {commercialSeatAccounts} from '@erpx/identity-access-server/public/commercial-seat-facts';
import {commercialSeatCandidates} from '@erpx/identity-access-server/public/commercial-seat-facts';
import 'server-only';
import {monthBounds} from '@erpx/billing-ui/lib/billing';
import {readNativeSeatHistory,type Lane37Actor} from './native-lane-37/client';
import {holdingMonthReadSql} from './commercial-read-tail-r7-sources';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {createSeatMeterApplication,type SeatMeterRepository} from '@erpx/commercial-server/application/seats';
import type {AssignmentEvent,MemberEvent,RoleEvent} from '@erpx/commercial-server/domain/seats';
export {canonicalSeatIntervals,deriveHoldingSeatIntervals} from '@erpx/commercial-server/domain/seats';
export type {SeatInterval,HoldingSeatMeter,ActiveSeatSnapshot,MonthlySeatSummary} from '@erpx/commercial-server/domain/seats';
/**
 * Native lane 37 owns the four seat-history SELECTs, their typed projection and
 * the derived Holding meter. The retained TypeScript derivation is used only
 * by the application-level repository path and emits no SQL; the live reader
 * below transports the owner response and fails closed without a local query.
 */
function repository(db:SqlDatabase,control:SqlDatabase,actor?:Lane37Actor):SeatMeterRepository{return {
 async history(start,end){const [native,cutoffs]=await Promise.all([
    readNativeSeatHistory(actor!,start,end,false),
    commercialSeatCutoffDependency(db,start,end),
  ]);
  const r=native.rows;
  return {historyStartedAt:r.historyStartedAt,
    assignments:r.assignments.map(row=>({event_id:row.eventId,assignment_id:row.assignmentId,captured_at:row.capturedAt,present:row.present,state_json:row.stateJson})) as AssignmentEvent[],
    members:r.members.map(row=>({event_id:row.eventId,company_id:row.companyId,user_id:row.userId,captured_at:row.capturedAt,present:row.present,state_json:row.stateJson})) as MemberEvent[],
    roles:r.roles.map(row=>({event_id:row.eventId,company_id:row.companyId,role_id:row.roleId,captured_at:row.capturedAt,present:row.present,state_json:row.stateJson})) as RoleEvent[],
    hasCutoffDependency:cutoffs?.dependency??true};},async activeCandidates(asOf){const holding=db;const candidates=(await commercialSeatCandidates(holding,asOf)).results;
   return candidates;},async accounts(ids){const rows=(await commercialSeatAccounts(control,ids)).results;
    return rows;},async snapshot(holdingId,month){const snapshot=await control.prepare(holdingMonthReadSql("snapshot_id,seat_coverage,seat_source_count","billing_usage_snapshots"))
    .bind(holdingId,month).first<{snapshot_id:string;seat_coverage:string;seat_source_count:number}>();
   return snapshot;},async summaryRows(holdingId,month){const rows=(await control.prepare(holdingMonthReadSql("intervals_json::text intervals_json,snapshot_id","billing_seat_usage"))
    .bind(holdingId,month).all<{intervals_json:string;snapshot_id:string}>()).results;
   return rows;}};}
export async function readHoldingSeatIntervals(db:SqlDatabase,actor:Lane37Actor,month:string){const {start,end}=monthBounds(month);if(end>Date.now())return {coverage:'unavailable' as const,users:[]};
  const cutoffs=await commercialSeatCutoffDependency(db,start,end);
  const {meter}=await readNativeSeatHistory(actor,start,end,cutoffs?.dependency??true);
  return meter;}
export function readActiveHoldingSeatCount(holding:SqlDatabase,control:SqlDatabase,asOf=Date.now()){return createSeatMeterApplication(repository(holding,control),{now:()=>Date.now()}).readActiveHoldingSeatCount(asOf);}
export function readControlSeatSummary(control:SqlDatabase,holdingId:string,month:string){return createSeatMeterApplication(repository(control,control),{now:()=>Date.now()}).readControlSeatSummary(holdingId,month);}
