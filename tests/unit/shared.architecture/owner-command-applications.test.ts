import { describe, expect, test } from "vitest";
import { createUsagePeriodApplication } from "@erpx/server-commercial/application/usage";
import { platformUsageItems, type UsageItem } from "@erpx/server-commercial/domain/usage";
import { agentSourceAllowed } from "@erpx/server-agent/domain/source-policy";

describe("owner time and Agent source policies", () => {
  test("uses the injected clock and Bangkok month boundary", () => {
    const now = Date.parse("2026-09-30T17:00:00.000Z");
    const select = createUsagePeriodApplication({ now: () => now });
    expect(select(null)).toMatchObject({ month: "2026-10", start: now, end: now + 1 });
    expect(() => select("2026-11")).toThrow("Choose this month or an earlier month");
    expect(() => select("2026-13")).toThrow();
  });
  test("preserves grouped platform usage quantities and source projections", () => {
    const base: UsageItem = { id: "source-1", holdingId: "holding", companyId: "company", companyName: "Company", branchId: null, branchName: null, type: "ai", quantity: 10, unit: "tokens", period: "2026-10", source: "private title", pricingStatus: "unpriced", estimatedSatang: null, inputTokens: 4, outputTokens: 6, occurredAt: 100 };
    const grouped = platformUsageItems([base, { ...base, id: "source-2", quantity: 20, inputTokens: 8, outputTokens: 12 }]);
    expect(grouped).toHaveLength(1);
    expect(grouped[0]).toMatchObject({ quantity: 30, inputTokens: 12, outputTokens: 18, source: "AI provider", occurredAt: undefined });
    expect(base.quantity).toBe(10);
  });
  test("Agent source scope uses segment and query boundaries", () => {
    expect(agentSourceAllowed("/work/item", ["/work"])).toBe(true);
    expect(agentSourceAllowed("/work?queue=todo", ["/work"])).toBe(true);
    expect(agentSourceAllowed("/workforce", ["/work"])).toBe(false);
    expect(agentSourceAllowed("/work?queue=done", ["/work?queue=todo"])).toBe(false);
    expect(agentSourceAllowed("/other", ["/"])).toBe(true);
  });
});
import { createBenefitsApplication, type BenefitsPorts } from "@erpx/server-benefits/application/benefits";
import type { BenefitRow } from "@erpx/server-benefits/domain/benefits";

function benefitsFixture(overrides: Partial<BenefitsPorts> = {}) {
 const calls:string[]=[];
 const person:BenefitRow={id:"worker",engagement_id:"engagement"};
 const ports:BenefitsPorts={
  actorId:"reviewer",canRead:true,canReview:true,canWrite:true,canManageDocuments:true,
  async authorize(){calls.push("authorize");},async person(){calls.push("person");return person;},
  async plans(){return [];},async windows(){return [];},async dependents(){return [];},async elections(){return [];},
  async election(){return null;},async history(){return [];},async payroll(){return null;},
  ...overrides,
 };
 return {app:createBenefitsApplication(ports),calls};
}

describe("Benefits owner application",()=>{
 test("personal visibility forces employee scope on the retained workspace read",async()=>{
  const f=benefitsFixture({async elections(personId,reviewer){expect(personId).toBe("worker");expect(reviewer).toBe(false);return [];}});
  expect((await f.app.workspace(true)).elections).toEqual([]);
  expect(f.calls).toEqual(["authorize","person"]);
 });
 test("detail does not expose another employee election to a broad reviewer",async()=>{
  const f=benefitsFixture();
  await expect(f.app.detail("election")).rejects.toMatchObject({code:"BENEFIT_ELECTION_NOT_FOUND",status:404});
 });
 test("work returns the owner queue without any command path",async()=>{
  const f=benefitsFixture({async elections(){return [{id:"election",plan_name:"Family health",employee_name:"Worker",status:"elected",updated_at:10,person_id:"worker"}];}});
  expect(await f.app.work()).toEqual([expect.objectContaining({id:"election",type:"benefit_enrollment",canAct:true,queue:"todo"})]);
 });
});
