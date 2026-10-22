import { beforeAll, expect, it } from "vitest";
import { retirementInstant, retirementLocalTime, nextRetirementDay } from "@erpx/feature-people_payroll/lib/retirement";
import { lifecycleInput } from "@erpx/feature-people_payroll/lib/people-lifecycle";
import { seedRetirement } from "../../../scripts/seed/seed-people-retirement";
import { env, applyMigrations } from "../../support/runtime-env";
beforeAll(async()=>{await applyMigrations(env.H001_DB,env.STAFF_TEST_MIGRATIONS);});
it("defaults cutoff to next-day Bangkok midnight, independent of browser timezone",()=>{
 const local=nextRetirementDay("2026-12-31");expect(local).toBe("2027-01-01T00:00");
 expect(retirementInstant(local,"Asia/Bangkok")).toBe(Date.parse("2026-12-31T17:00:00Z"));
 expect(retirementLocalTime(Date.parse("2026-12-31T17:00:00Z"),"Asia/Bangkok")).toBe(local);
 expect(()=>retirementInstant("2026-03-08T02:30","America/New_York")).toThrow();
 expect(()=>retirementInstant("2026-11-01T01:30","America/New_York")).toThrow();
});
it("keeps legacy departures as other",()=>{
 expect(lifecycleInput.parse({kind:"offboarding",expectedVersion:0,personId:"p",engagementId:"e",positionId:null,effectiveAt:1,reason:"Legacy departure"}).departureReason).toBe("other");
});
it("seeds six fixed fictional records without changing them on repeat",async()=>{
 const first=await seedRetirement(env.H001_DB,"fictional","2026-09-17");expect(first.filter(x=>x.created)).toHaveLength(6);
 const before=await env.H001_DB.prepare("SELECT definition FROM people_lifecycle_cases ORDER BY id").all();
 const second=await seedRetirement(env.H001_DB,"fictional","2026-10-10");expect(second.some(x=>x.created)).toBe(false);
 expect((await env.H001_DB.prepare("SELECT definition FROM people_lifecycle_cases ORDER BY id").all()).results).toEqual(before.results);
 expect(await env.H001_DB.prepare("SELECT count(*) n FROM staff_notifications").first("n")).toBe(0);
});
