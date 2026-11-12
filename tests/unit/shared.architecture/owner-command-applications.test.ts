import { describe, expect, test } from "vitest";
import { createTalentRequisitionApplication, type TalentRequisitionInputPorts } from "@erpx/server-talent/application/requisition";
import { createUsagePeriodApplication } from "@erpx/server-commercial/application/usage";
import { platformUsageItems, type UsageItem } from "@erpx/server-commercial/domain/usage";
import { agentSourceAllowed } from "@erpx/server-agent/domain/source-policy";

const requisition = { jobTitle: "Engineer", openings: 2, targetStartDate: "2026-10-31", description: "Maintain the company service" };
type Command = Parameters<TalentRequisitionInputPorts["persist"]>[0];
function fixture() {
  const calls: string[] = [];
  const commands: Command[] = [];
  const ports = {
    clock: { now: () => 42 }, branchId: "branch-1" as string | null,
    async authorize() { calls.push("authorize"); },
    async replay(): Promise<Record<string, unknown> | null> { calls.push("replay"); return null; },
    async persist(command: Command): Promise<Record<string, unknown>> { commands.push(command); return command.result; },
  };
  return { calls, commands, ports };
}

describe("owner draft commands without PostgreSQL", () => {
  test("Talent requires Branch scope before receipt lookup and persists a reviewable requisition", async () => {
    const f = fixture();
    f.ports.branchId = null;
    await expect(createTalentRequisitionApplication(f.ports)(requisition, "key-123456")).rejects.toMatchObject({ code: "BRANCH_SCOPE_REQUIRED" });
    expect(f.calls).toEqual(["authorize"]);
    expect(f.commands).toEqual([]);
    f.ports.branchId = "branch-1";
    expect(await createTalentRequisitionApplication(f.ports)(requisition, "key-123456")).toEqual({ id: "talent-req-key-123456", status: "draft", version: 1 });
  });
  test("permission failures and replay do not persist a second draft", async () => {
    const f = fixture();
    f.ports.authorize = async () => { throw new Error("authority revoked"); };
    await expect(createTalentRequisitionApplication(f.ports)(requisition, "key-123456")).rejects.toThrow("authority revoked");
    expect(f.commands).toEqual([]);
    f.ports.authorize = async () => {};
    f.ports.replay = async () => ({ id: "original-requisition", status: "draft", version: 1 });
    expect(await createTalentRequisitionApplication(f.ports)(requisition, "key-123456")).toMatchObject({ id: "original-requisition" });
    expect(f.commands).toEqual([]);
  });
});

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
