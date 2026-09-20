import {it,expect} from 'vitest';
import {addBusinessTime,elapsedBusinessTime,businessCalendarSchema,type BusinessCalendar} from '../src/lib/business-time';
const clock:BusinessCalendar={timezone:'Asia/Bangkok',weekdays:[1,2,3,4,5],opensAtMinute:540,closesAtMinute:1020,holidays:['2026-09-21']};
const at=(s:string)=>Date.parse(s);
it('retains seconds and skips weekends/holidays according to the supplied calendar snapshot',()=>{
 const start=at('2026-09-18T09:59:30Z'),due=addBusinessTime(clock,start,120000);
 expect(new Date(due).toISOString()).toBe('2026-09-22T02:01:30.000Z');
 expect(elapsedBusinessTime(clock,start,due)).toBe(120000);
 expect(addBusinessTime({...clock,holidays:[]},start,120000)).toBe(at('2026-09-21T02:01:30Z'));
 expect(clock.holidays).toEqual(['2026-09-21']);
});
it('counts actual working time across skipped and repeated local DST hours',()=>{
 const policy:BusinessCalendar={timezone:'Europe/Berlin',weekdays:[0],opensAtMinute:60,closesAtMinute:240,holidays:[]};
 expect(elapsedBusinessTime(policy,at('2026-03-29T00:00:00Z'),at('2026-03-29T02:00:00Z'))).toBe(2*3600000);
 expect(addBusinessTime(policy,at('2026-03-29T00:00:00Z'),90*60000)).toBe(at('2026-03-29T01:30:00Z'));
 expect(elapsedBusinessTime(policy,at('2026-10-24T23:00:00Z'),at('2026-10-25T03:00:00Z'))).toBe(4*3600000);
 expect(addBusinessTime(policy,at('2026-10-24T23:00:00Z'),3*3600000)).toBe(at('2026-10-25T02:00:00Z'));
});
it('validates timezone, weekday uniqueness, daytime windows and clock boundaries',()=>{
 expect(businessCalendarSchema.safeParse({...clock,timezone:'Missing/Timezone'}).success).toBe(false);
 expect(businessCalendarSchema.safeParse({...clock,weekdays:[1,1]}).success).toBe(false);
 expect(businessCalendarSchema.safeParse({...clock,closesAtMinute:clock.opensAtMinute}).success).toBe(false);
 expect(()=>elapsedBusinessTime(clock,1000,999)).toThrow();
 expect(addBusinessTime(clock,at('2026-09-20T00:00:00Z'),0)).toBe(at('2026-09-20T00:00:00Z'));
});
