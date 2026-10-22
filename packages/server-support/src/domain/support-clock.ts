import {z} from "zod";
import {businessCalendarSchema,addBusinessTime,elapsedBusinessTime} from "@erpx/feature-shared/lib/business-time";
export const supportSlaPolicySchema=z.object({calendar:businessCalendarSchema,targetMinutes:z.number().int().min(1).max(28800),pauseWhileWaiting:z.boolean()});
export type SupportSlaPolicy=z.infer<typeof supportSlaPolicySchema>;
export type SupportClock={snapshot:SupportSlaPolicy;remainingMs:number;startedAt:number|null;dueAt:number|null;state:'running'|'paused'|'stopped';breachedAt:number|null};
export function startSupportClock(raw:SupportSlaPolicy,now:number):SupportClock{const snapshot=supportSlaPolicySchema.parse(raw),remainingMs=snapshot.targetMinutes*60000;return {snapshot,remainingMs,startedAt:now,dueAt:addBusinessTime(snapshot.calendar,now,remainingMs),state:'running',breachedAt:null};}
/** Ticket owner invokes this inside its versioned command; never advances a ticket itself. */
export function advanceSupportClock(clock:SupportClock,action:string,now:number):SupportClock{
 if(!Number.isSafeInteger(now)||now<0||(clock.startedAt!==null&&now<clock.startedAt))throw new Error('Support clock cannot move backwards.');
 const next={...clock,snapshot:supportSlaPolicySchema.parse(clock.snapshot)};
 if(clock.state==='running'&&clock.startedAt!==null&&clock.dueAt!==null){
  if(clock.remainingMs>0)next.remainingMs=Math.max(0,clock.remainingMs-elapsedBusinessTime(clock.snapshot.calendar,clock.startedAt,Math.max(clock.startedAt,Math.min(now,clock.dueAt))));
  if(now>clock.dueAt)next.breachedAt??=clock.dueAt;
  next.startedAt=now;
 }
 if(['resolve','cancel'].includes(action)){next.state='stopped';next.startedAt=null;}
 else if(action==='wait-customer'&&clock.snapshot.pauseWhileWaiting){next.state='paused';next.startedAt=null;if(next.breachedAt===null)next.dueAt=null;}
 else if(['resume','reopen','reject-resolution'].includes(action)&&clock.state!=='running'){
  next.state='running';next.startedAt=now;next.dueAt=next.breachedAt??addBusinessTime(next.snapshot.calendar,now,next.remainingMs);
 }
 return next;
}
