import { z } from "zod";
export const clearanceKinds = ["hr", "handover", "equipment", "directory", "payroll", "finance"] as const;
export type ClearanceKind = typeof clearanceKinds[number];
export const clearanceLabels: Record<ClearanceKind,string> = {hr:"Review HR records and documents",handover:"Hand over work and pending approvals",equipment:"Return equipment and assets",directory:"End access and verify directory",payroll:"Review final payroll",finance:"Clear expenses, advances and outstanding balances"};
export const clearancePermissions: Record<ClearanceKind,string | null> = {hr:"people.manage",handover:null,equipment:null,directory:"iam.directory.manage",payroll:"payroll.prepare",finance:"finance.write"};
export const clearanceReference = z.object({kind:z.enum(["payroll","expense","invoice","journal"]),id:z.string().min(1).max(180)}).strict();
export type ClearanceReference = z.infer<typeof clearanceReference>;
export const clearanceCompletion = {status:z.enum(["completed","not_required"]).default("completed"),reason:z.string().trim().max(2000).default(""),references:z.array(clearanceReference).max(30).default([])};
export function retirementLocalTime(timestamp:number,timezone:string) {
 const parts = new Intl.DateTimeFormat("en-CA",{timeZone:timezone,year:"numeric",month:"2-digit",day:"2-digit",hour:"2-digit",minute:"2-digit",hourCycle:"h23"}).formatToParts(timestamp);
 const get=(name:string)=>parts.find(p=>p.type===name)!.value;
 return `${get("year")}-${get("month")}-${get("day")}T${get("hour")}:${get("minute")}`;
}
/** Reject nonexistent and ambiguous DST wall times; HR must choose an unambiguous cutoff. */
export function retirementInstant(local:string,timezone:string) {
 const guess=Date.parse(`${local}:00Z`);
 if (!Number.isFinite(guess)) throw Error("Choose a valid cutoff time.");
 let instant=guess;
 for(let i=0;i<4;i++) instant+=guess-Date.parse(`${retirementLocalTime(instant,timezone)}:00Z`);
 if(retirementLocalTime(instant,timezone)!==local)throw Error("This local time does not exist in the selected timezone.");
 if([-120,-90,-60,-30,30,60,90,120].some(m=>retirementLocalTime(instant+m*60000,timezone)===local))throw Error("This local time occurs twice. Choose an unambiguous cutoff time.");
 return instant;
}
export function nextRetirementDay(date:string) {return new Date(Date.parse(`${date}T00:00:00Z`)+86400000).toISOString().slice(0,10)+"T00:00";}
