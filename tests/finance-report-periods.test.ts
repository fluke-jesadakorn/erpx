import { expect,it } from "vitest";
import { financeReportPeriods } from "../src/lib/finance-report-periods";
const period=(id:string,start_date:string,end_date:string,state="closed")=>({id,start_date,end_date,state});
it("uses the closed fiscal year without double selecting its closed monthly periods",()=>{
 expect(financeReportPeriods([period("September","2026-09-01","2026-09-30"),period("October","2026-10-01","2026-10-31"),period("year","2026-01-01","2026-12-31")],"2026-01-01","2026-12-31")).toEqual(["year"]);
});
it("covers consecutive months and refuses a missing or open source period",()=>{
 const rows=[period("September","2026-09-01","2026-09-30"),period("October","2026-10-01","2026-10-31")];
 expect(financeReportPeriods(rows,"2026-09-01","2026-10-31")).toEqual(["September","October"]);
 expect(financeReportPeriods([rows[0],{...rows[1],state:"open"}],"2026-09-01","2026-10-31")).toEqual([]);
 expect(financeReportPeriods(rows,"2026-08-01","2026-10-31")).toEqual([]);
});
it("finds the continuous alternative when a longer reviewed period leaves a gap",()=>{
 expect(financeReportPeriods([period("dead-end","2026-01-01","2026-01-20"),period("first","2026-01-01","2026-01-15"),period("second","2026-01-16","2026-01-31")],"2026-01-01","2026-01-31")).toEqual(["first","second"]);
});
it("keeps leap-day continuity and never suggests a partial reporting interval",()=>{
 const rows=[period("February","2028-02-01","2028-02-29"),period("March","2028-03-01","2028-03-31")];
 expect(financeReportPeriods(rows,"2028-02-01","2028-03-31")).toEqual(["February","March"]);
 expect(financeReportPeriods(rows,"2028-02-02","2028-03-31")).toEqual([]);
});
