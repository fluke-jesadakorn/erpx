import {monthBounds} from "@erpx/feature-billing/lib/billing";
import type {Clock} from '@erpx/server-shared/index';
import {deriveHoldingSeatIntervals,summarizeControlSeats,type HoldingSeatMeter,type ActiveSeatSnapshot,type MonthlySeatSummary,type AssignmentEvent,type MemberEvent,type RoleEvent,type SeatSnapshotHeader,type SeatSummaryRow,type SeatControlAccount} from '@erpx/server-commercial/domain/seats';
export interface SeatMeterRepository{
 history(start:number,end:number):Promise<{historyStartedAt:number|null;assignments:AssignmentEvent[];members:MemberEvent[];roles:RoleEvent[];hasCutoffDependency:boolean}>;
 activeCandidates(asOf:number):Promise<{user_id:string}[]>;accounts(ids:string[]):Promise<SeatControlAccount[]>;
 snapshot(holdingId:string,month:string):Promise<SeatSnapshotHeader|null>;summaryRows(holdingId:string,month:string):Promise<SeatSummaryRow[]>;
}
export function createSeatMeterApplication(repository:SeatMeterRepository,clock:Clock){return {
 async readHoldingSeatIntervals(month:string):Promise<HoldingSeatMeter>{const {start,end}=monthBounds(month);if(end>clock.now())return {coverage:'unavailable',users:[]};return deriveHoldingSeatIntervals({month,...await repository.history(start,end)});},
 async readActiveHoldingSeatCount(asOf:number):Promise<ActiveSeatSnapshot>{const candidates=await repository.activeCandidates(asOf);if(!candidates.length)return {status:'measured',count:0,asOf};let count=0;for(let offset=0;offset<candidates.length;offset+=500){const ids=candidates.slice(offset,offset+500).map(row=>row.user_id),rows=await repository.accounts(ids);if(rows.some(row=>row.status===null))return {status:'unavailable',count:null,asOf:null};count+=rows.filter(row=>row.status==='active'&&!row.platform_identity).length;}return {status:'measured',count,asOf};},
 async readControlSeatSummary(holdingId:string,month:string):Promise<MonthlySeatSummary>{const snapshot=await repository.snapshot(holdingId,month);if(snapshot?.seat_coverage!=='measured')return {status:'unavailable',distinctUsers:null,activeSeatMilliseconds:null};return summarizeControlSeats(month,snapshot,await repository.summaryRows(holdingId,month));},
};}

