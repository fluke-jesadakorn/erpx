import {it,expect} from 'vitest';
import {startSupportClock,advanceSupportClock,type SupportSlaPolicy} from "@erpx/server-support/public/rules/support-clock";
const at=(s:string)=>Date.parse(s),policy:SupportSlaPolicy={calendar:{timezone:'Asia/Bangkok',weekdays:[1,2,3,4,5],opensAtMinute:540,closesAtMinute:1020,holidays:[]},targetMinutes:60,pauseWhileWaiting:true};
it('pauses only when the snapshot permits and resumes with the unspent working duration',()=>{
 const original=startSupportClock(policy,at('2026-09-18T09:30:00Z'));
 expect(original.dueAt).toBe(at('2026-09-21T02:30:00Z'));
 const paused=advanceSupportClock(original,'wait-customer',at('2026-09-18T09:45:00Z'));
 expect(paused).toMatchObject({state:'paused',remainingMs:45*60000,dueAt:null});
 const resumed=advanceSupportClock(paused,'resume',at('2026-09-22T02:00:00Z'));
 expect(resumed.dueAt).toBe(at('2026-09-22T02:45:00Z'));
 expect(original.remainingMs).toBe(60*60000);
 const resolved=advanceSupportClock(resumed,'resolve',at('2026-09-22T02:30:00Z'));
 expect(resolved).toMatchObject({state:'stopped',remainingMs:15*60000,breachedAt:null});
 expect(advanceSupportClock(resolved,'reopen',at('2026-09-23T02:00:00Z')).dueAt).toBe(at('2026-09-23T02:15:00Z'));
});
it('preserves a breach through later resolution and reopening and does not reset the budget',()=>{
 const c=startSupportClock({...policy,pauseWhileWaiting:false},at('2026-09-21T02:00:00Z'));
 const waiting=advanceSupportClock(c,'wait-customer',at('2026-09-21T02:30:00Z'));
 expect(waiting.state).toBe('running');
 const late=advanceSupportClock(waiting,'resolve',at('2026-09-22T02:00:00Z'));
 expect(late).toMatchObject({breachedAt:at('2026-09-21T03:00:00Z'),remainingMs:0,state:'stopped'});
 const reopened=advanceSupportClock(late,'reopen',at('2026-09-23T02:00:00Z'));
 expect(advanceSupportClock(reopened,'resolve',at('2026-09-24T02:00:00Z'))).toMatchObject({breachedAt:late.breachedAt,remainingMs:0});
});
it('accepts completion exactly on the target, rejects backwards time and keeps snapshots independent',()=>{
 const c=startSupportClock(policy,at('2026-09-21T02:00:00Z'));
 expect(advanceSupportClock(c,'resolve',c.dueAt!)).toMatchObject({remainingMs:0,breachedAt:null});
 expect(()=>advanceSupportClock(c,'resolve',c.startedAt!-1)).toThrow('backwards');
 policy.calendar.holidays.push('2026-09-21');
 expect(c.snapshot.calendar.holidays).toEqual([]);policy.calendar.holidays=[];
});
