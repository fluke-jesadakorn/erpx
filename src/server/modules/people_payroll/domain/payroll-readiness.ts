import {z} from 'zod';import {BusinessRuleError} from '../../shared';export const payrollScheduleSchema = z.object({
  enabled: z.boolean(),
  payDelayDays: z.number().int().min(1).max(45),
  anchorDate: z.string().date().nullable().optional(),
  expectedVersion: z.number().int().positive(),
}).strict();
export const payrollSubmitReviewSchema = z.object({
  expectedVersion: z.number().int().positive(),
  inputDigest: z.string().regex(/^[a-f0-9]{64}$/),
}).strict();
export type Schedule = { id:string; company_id:string; currency:string; country_code:string; frequency:string; timezone:string; schedule_enabled:boolean; pay_delay_days:number; schedule_anchor_date:string|null; schedule_version:number };
export type Run = { id:string; company_id:string; pay_group_id:string; country_pack_id:string|null; scope_engagement_id:string|null; period_start:string; period_end:string; pay_date:string; currency:string; mode:string; status:string; version:number; revision:number; input_digest:string|null; submitted_by:string|null; submitted_digest:string|null;adjustment_of_run_id:string|null };
export type PayrollBlocker = {code:string; message:string; href?:string};
export function localDate(now:Date, timezone:string) {
  try {
    const parts = new Intl.DateTimeFormat("en-CA",{timeZone:timezone,year:"numeric",month:"2-digit",day:"2-digit"}).formatToParts(now);
    const part=(type:string)=>parts.find(value=>value.type===type)?.value;
    return `${part("year")}-${part("month")}-${part("day")}`;
  } catch { throw new BusinessRuleError(422,"PAYROLL_TIMEZONE_INVALID","Set a valid pay group time zone."); }
}
export function equal(a:unknown,b:unknown):boolean {
  const normalized=(value:unknown):unknown=>Array.isArray(value)?value.map(normalized):value&&typeof value==="object"?Object.fromEntries(Object.entries(value).sort(([a],[b])=>a.localeCompare(b)).map(([key,entry])=>[key,normalized(entry)])):value;
  return JSON.stringify(normalized(a))===JSON.stringify(normalized(b));
}
