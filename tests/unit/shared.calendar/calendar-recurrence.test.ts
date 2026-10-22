import {describe,expect,it} from "vitest";
import {expandCalendarSeries} from "@erpx/feature-work_projection/lib/calendar-recurrence";
import {localMinuteAt} from "@erpx/feature-work_projection/lib/calendar-timeline";

const from=(date:string)=>Date.parse(`${date}T00:00:00Z`);
describe("calendar recurrence",()=>{
  it("keeps local wall-clock time and skips the spring DST gap",()=>{
    const rows=expandCalendarSeries("2026-03-07T02:30","2026-03-07T03:00","America/New_York",
      {frequency:"daily",intervalCount:1,weekdays:[],endKind:"count",occurrenceCount:4},from("2026-03-07"),from("2026-03-12"));
    expect(rows.map(row=>row.key)).toEqual(["2026-03-07T02:30","2026-03-09T02:30","2026-03-10T02:30"]);
    expect(rows.map(row=>localMinuteAt(row.startsAt,"America/New_York").time)).toEqual(["02:30","02:30","02:30"]);
  });
  it("uses stable occurrence keys for weekly and monthly rules",()=>{
    const weekly=expandCalendarSeries("2026-01-05T09:00","2026-01-05T10:00","Asia/Bangkok",
      {frequency:"weekly",intervalCount:1,weekdays:[1,3],endKind:"count",occurrenceCount:4},from("2026-01-01"),from("2026-02-01"));
    expect(weekly.map(row=>row.key)).toEqual(["2026-01-05T09:00","2026-01-07T09:00","2026-01-12T09:00","2026-01-14T09:00"]);
    const monthly=expandCalendarSeries("2026-01-31T09:00","2026-01-31T10:00","Asia/Bangkok",
      {frequency:"monthly",intervalCount:1,weekdays:[],endKind:"count",occurrenceCount:3},from("2026-01-01"),from("2026-05-01"));
    expect(monthly.map(row=>row.key)).toEqual(["2026-01-31T09:00","2026-03-31T09:00"]);
  });
});
