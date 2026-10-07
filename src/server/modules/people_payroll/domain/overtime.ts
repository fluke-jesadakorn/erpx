import {BusinessRuleError} from '../../shared';import type {OvertimeDetails} from '@/lib/overtime';export const fail = (code: string, message: string, status=409): never => { throw new BusinessRuleError(status,code,message); };
export type OvertimeRow = {
 id:string; company_id:string; engagement_id:string; person_id:string; branch_id:string|null; department_id:string|null;
 created_by:string; mode:"advance"|"retrospective"; state:string; timezone:string; start_local:string; end_local:string;
 starts_at:number; ends_at:number; break_minutes:number; minutes:number; reason:string; late_explanation:string;
 roster_shift_id:string|null; planned_snapshot:string|null; actuals_snapshot:string|null; decision_note:string;
 version:number; created_at:number; updated_at:number;
};
export const detailsOf=(r:OvertimeRow):OvertimeDetails=>({mode:r.mode,startLocal:r.start_local,endLocal:r.end_local,timezone:r.timezone,breakMinutes:r.break_minutes,reason:r.reason,lateExplanation:r.late_explanation,rosterShiftId:r.roster_shift_id});
