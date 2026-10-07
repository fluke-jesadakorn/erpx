import {BusinessRuleError} from '../../shared';import type {RosterWeek,RosterShift} from './roster';export const fail = (code: string, message: string, status = 409): never => { throw new BusinessRuleError(status, code, message); };
export type Result = { id: string; version: number; state?: string };
export type ShiftWithWeek = RosterShift & { company_id: string; branch_id: string | null; department_id: string; week_start: string; timezone: string; week_state: RosterWeek["state"] };
export function expected(actual: number, supplied: number | undefined) { if (actual !== supplied) fail("ROSTER_CHANGED", "This roster changed. Refresh before trying again."); }
export function future(s:RosterShift,now:number) { if (s.starts_at <= now) fail("ROSTER_STARTED", "A shift that has started can no longer be changed."); }
export function publishedReason(w: RosterWeek | ShiftWithWeek, reason: string) {
  if (("week_state" in w ? w.week_state : w.state) === "published" && reason.trim().length < 3) fail("ROSTER_REASON", "Explain the change to this published roster.", 422);
}
export type RosterGap = { person_id: string; legal_name: string; engagement_id: string; week_start: string };
