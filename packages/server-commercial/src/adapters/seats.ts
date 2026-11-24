import {commercialSeatCutoffDependency} from '@erpx/server-people_payroll/public/commercial-seat-cutoff';
import {commercialSeatAccounts} from '@erpx/server-identity_access/public/commercial-seat-facts';
import {commercialSeatCandidates} from '@erpx/server-identity_access/public/commercial-seat-facts';
import 'server-only';
import {seatHistorySql} from './commercial-read-tail-r3-sources';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {createSeatMeterApplication,type SeatMeterRepository} from '@erpx/server-commercial/application/seats';
import type {AssignmentEvent,MemberEvent,RoleEvent} from '@erpx/server-commercial/domain/seats';
export {canonicalSeatIntervals,deriveHoldingSeatIntervals} from '@erpx/server-commercial/domain/seats';
export type {SeatInterval,HoldingSeatMeter,ActiveSeatSnapshot,MonthlySeatSummary} from '@erpx/server-commercial/domain/seats';
function repository(db:SqlDatabase,control:SqlDatabase):SeatMeterRepository{return {
async history(start,end){const [coverage,assignments,members,roles,cutoffs]=await Promise.all([
    db.prepare("SELECT started_at FROM billing_seat_history_coverage WHERE singleton=1").first<{started_at:number}>(),
    db.prepare(seatHistorySql("assignment_id","assignment","assignment_id"))
      .bind(end).all<AssignmentEvent>(),
    db.prepare(seatHistorySql("company_id,user_id","member","company_id,user_id"))
      .bind(end).all<MemberEvent>(),
    db.prepare(seatHistorySql("company_id,role_id","role","company_id,role_id"))
      .bind(end).all<RoleEvent>(),
    commercialSeatCutoffDependency(db,start,end),
  ]);
  return {historyStartedAt:coverage?.started_at??null,assignments:assignments.results,members:members.results,roles:roles.results,hasCutoffDependency:cutoffs?.dependency??true};},async activeCandidates(asOf){const holding=db;const candidates=(await commercialSeatCandidates(holding,asOf)).results;
  return candidates;},async accounts(ids){const rows=(await commercialSeatAccounts(control,ids)).results;
    return rows;},async snapshot(holdingId,month){const snapshot=await control.prepare(`SELECT snapshot_id,seat_coverage,seat_source_count
    FROM billing_usage_snapshots WHERE holding_id=? AND month=?`)
    .bind(holdingId,month).first<{snapshot_id:string;seat_coverage:string;seat_source_count:number}>();
  return snapshot;},async summaryRows(holdingId,month){const rows=(await control.prepare(`SELECT intervals_json::text intervals_json,snapshot_id
    FROM billing_seat_usage WHERE holding_id=? AND month=?`)
    .bind(holdingId,month).all<{intervals_json:string;snapshot_id:string}>()).results;
  return rows;}};}
export function readHoldingSeatIntervals(db:SqlDatabase,month:string){return createSeatMeterApplication(repository(db,db),{now:()=>Date.now()}).readHoldingSeatIntervals(month);}
export function readActiveHoldingSeatCount(holding:SqlDatabase,control:SqlDatabase,asOf=Date.now()){return createSeatMeterApplication(repository(holding,control),{now:()=>Date.now()}).readActiveHoldingSeatCount(asOf);}
export function readControlSeatSummary(control:SqlDatabase,holdingId:string,month:string){return createSeatMeterApplication(repository(control,control),{now:()=>Date.now()}).readControlSeatSummary(holdingId,month);}



