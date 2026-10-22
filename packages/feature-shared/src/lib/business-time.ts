import {z} from 'zod';
export const businessCalendarSchema=z.object({
 timezone:z.string().max(80).refine(v=>{try{new Intl.DateTimeFormat('en',{timeZone:v});return true;}catch{return false;}},'Choose an IANA timezone.'),
 weekdays:z.array(z.number().int().min(0).max(6)).min(1).max(7).refine(v=>new Set(v).size===v.length,'Choose each weekday once.'),
 opensAtMinute:z.number().int().min(0).max(1439),closesAtMinute:z.number().int().min(1).max(1440),
 holidays:z.array(z.iso.date()).max(366).default([]),
}).refine(v=>v.closesAtMinute>v.opensAtMinute,'Closing time must follow opening time within the same day.');
export type BusinessCalendar=z.infer<typeof businessCalendarSchema>;
const weekdays:Record<string,number>={Sun:0,Mon:1,Tue:2,Wed:3,Thu:4,Fri:5,Sat:6};
const MAX_HORIZON=366*86400000;
/** Minute-aligned calendar windows, exact elapsed milliseconds within each window.
 * Walking UTC windows respects skipped/repeated local hours without inventing times.
 * Policies are caller-owned immutable snapshots, never inferred from the locale.
 */
function predicate(raw:BusinessCalendar){const p=businessCalendarSchema.parse(raw),holidays=new Set(p.holidays),days=new Set(p.weekdays),formatter=new Intl.DateTimeFormat('en-GB',{timeZone:p.timezone,year:'numeric',month:'2-digit',day:'2-digit',weekday:'short',hour:'2-digit',minute:'2-digit',hourCycle:'h23'});return (at:number)=>{const parts=Object.fromEntries(formatter.formatToParts(at).map(p=>[p.type,p.value])),date=`${parts.year}-${parts.month}-${parts.day}`,minute=Number(parts.hour)*60+Number(parts.minute);return days.has(weekdays[parts.weekday])&&!holidays.has(date)&&minute>=p.opensAtMinute&&minute<p.closesAtMinute;};}
function timestamp(value:number){if(!Number.isSafeInteger(value)||value<0||!Number.isFinite(new Date(value).getTime()))throw new Error('Invalid business clock timestamp.');}
export function addBusinessTime(policy:BusinessCalendar,start:number,durationMs:number){timestamp(start);if(!Number.isSafeInteger(durationMs)||durationMs<0||durationMs>MAX_HORIZON)throw new Error('Invalid business clock duration.');if(durationMs===0)return start;const working=predicate(policy),end=start+MAX_HORIZON;let cursor=start,remaining=durationMs;while(cursor<end){const next=Math.min(Math.floor(cursor/60000)*60000+60000,end);if(working(cursor)){const available=next-cursor;if(remaining<=available)return cursor+remaining;remaining-=available;}cursor=next;}throw new Error('This calendar cannot fit the target within 366 days.');}
export function elapsedBusinessTime(policy:BusinessCalendar,start:number,end:number){timestamp(start);timestamp(end);if(end<start||end-start>MAX_HORIZON)throw new Error('Business clock interval must be between zero and 366 days.');const working=predicate(policy);let cursor=start,total=0;while(cursor<end){const next=Math.min(Math.floor(cursor/60000)*60000+60000,end);if(working(cursor))total+=next-cursor;cursor=next;}return total;}
