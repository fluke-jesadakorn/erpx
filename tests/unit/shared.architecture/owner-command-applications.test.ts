import { describe, expect, test } from "vitest";
import { createBenefitPlanApplication, type BenefitPlanInputPorts } from "@erpx/server-benefits/application/plan";
import { createLearningCourseApplication, type LearningCourseInputPorts } from "@erpx/server-learning/application/course";
import { createTalentRequisitionApplication, type TalentRequisitionInputPorts } from "@erpx/server-talent/application/requisition";
import { createUsagePeriodApplication } from "@erpx/server-commercial/application/usage";
import { platformUsageItems, type UsageItem } from "@erpx/server-commercial/domain/usage";
import { agentSourceAllowed } from "@erpx/server-agent/domain/source-policy";

const plan = { name: "Family health", provider: "Provider", coverageType: "family", currency: "thb", employeeDeduction: "100.25", employerCost: "200.50", effectiveFrom: "2026-10-01", effectiveTo: "2026-10-31" };
const course = { title: "Safety course", description: "Workplace safety training", provider: "Provider", durationMinutes: 120 };
const requisition = { jobTitle: "Engineer", openings: 2, targetStartDate: "2026-10-31", description: "Maintain the company service" };
type Command = Parameters<BenefitPlanInputPorts["persist"]>[0] | Parameters<LearningCourseInputPorts["persist"]>[0] | Parameters<TalentRequisitionInputPorts["persist"]>[0];
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
  test("Benefits owns normalized plan input, effective range, clock and receipt order", async () => {
    const f = fixture();
    const result = await createBenefitPlanApplication(f.ports)(plan, "key-123456");
    expect(result).toEqual({ id: "benefit-plan-key-123456", status: "draft", version: 1 });
    expect(f.commands[0]).toMatchObject({ operation: "benefits.plan.create", now: 42, input: { currency: "THB", employeeDeduction: "100.25" } });
    expect(f.calls).toEqual(["authorize", "replay"]);
    await expect(createBenefitPlanApplication(f.ports)({ ...plan, effectiveTo: "2026-09-30" }, "other-key")).rejects.toMatchObject({ code: "INVALID_EFFECTIVE_RANGE", status: 422 });
    expect(f.commands).toHaveLength(1);
  });
  test("Learning validates course duration and emits the original draft result", async () => {
    const f = fixture();
    await expect(createLearningCourseApplication(f.ports)({ ...course, durationMinutes: 0 }, "key-123456")).rejects.toThrow();
    expect(f.commands).toEqual([]);
    expect(await createLearningCourseApplication(f.ports)(course, "key-123456")).toEqual({ id: "learning-course-key-123456", status: "draft", version: 1 });
    expect(f.commands[0]).toMatchObject({ operation: "learning.course.create", now: 42, input: course });
  });
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
    await expect(createLearningCourseApplication(f.ports)(course, "key-123456")).rejects.toThrow("authority revoked");
    expect(f.commands).toEqual([]);
    f.ports.authorize = async () => {};
    f.ports.replay = async () => ({ id: "original-course", status: "draft", version: 1 });
    expect(await createLearningCourseApplication(f.ports)(course, "key-123456")).toMatchObject({ id: "original-course" });
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
import { createBenefitsApplication, type BenefitsPorts, type BenefitMutation } from "@erpx/server-benefits/application/benefits";
import type { BenefitRow } from "@erpx/server-benefits/domain/benefits";

function benefitsFixture(overrides: Partial<BenefitsPorts<string, string>> = {}) {
 const calls:string[]=[],mutations:BenefitMutation<string,string>[]=[];
 const person:BenefitRow={id:"worker",engagement_id:"engagement"};
 const ports:BenefitsPorts<string,string>={
  actorId:"reviewer",clock:{now:()=>Date.UTC(2026,9,5)},canRead:true,canReview:true,canWrite:true,canManageDocuments:true,
  async authorize(){calls.push("authorize");},async authorizeReview(){calls.push("review");},
  async replay(){calls.push("replay");return null;},async person(){calls.push("person");return person;},
  async plans(){return [];},async windows(){return [];},async dependents(){return [];},async elections(){return [];},
  async plan(){return {id:"plan",coverage_type:"family",required_eligibility_code:"safety"};},async window(){return {opens_on:"2026-10-01",closes_on:"2026-10-31",effective_on:"2026-11-01"};},
  async verifiedDependents(){return ["dependent"];},async record(){return {id:"election",status:"submitted",version:2,created_by:"maker",person_id:"worker"};},
  async election(){return null;},async history(){return [];},async payroll(){return null;},
  async prepareLearning(){calls.push("prepare-learning");return "learning-receipt";},async validateLearning(){calls.push("validate-learning");return "learning-guards";},
  async prepareEvidence(){calls.push("evidence");return "file-binding";},
  async commit(mutation,result){mutations.push(mutation);calls.push("commit");return result;},...overrides,
 };
 return {app:createBenefitsApplication(ports),calls,mutations};
}

describe("Benefits owner application",()=>{
 test("replays a window before date validation but requires review authority",async()=>{
  const f=benefitsFixture({async replay(){return {id:"saved"};}});
  expect(await f.app.createWindow({name:"Annual enrollment",opensOn:"2026-10-31",closesOn:"2026-10-01",effectiveOn:"2026-11-01"},"key")).toEqual({id:"saved"});
  expect(f.calls).toEqual(["authorize","review"]);expect(f.mutations).toEqual([]);
 });
 test("dependent creation retains the active worker prerequisite before replay",async()=>{
  const f=benefitsFixture({async person(){return null;},async replay(){throw new Error("replay must follow worker prerequisite");}});
  await expect(f.app.createDependent({name:"Dependent",relationship:"child",birthDate:"2020-01-01"},"key")).rejects.toMatchObject({code:"BENEFIT_WORKER_REQUIRED"});
 });
 test("election replay does not read changed worker or source state",async()=>{
  const f=benefitsFixture({async replay(){return {id:"saved"};},async person(){throw new Error("must not read");}});
  expect(await f.app.createElection({planId:"plan",planVersion:1,windowId:"window",windowVersion:1,dependentIds:[]},"key")).toEqual({id:"saved"});expect(f.mutations).toEqual([]);
 });
 test("stages current Learning receipt with the elected coverage in one mutation",async()=>{
  const f=benefitsFixture();
  await f.app.createElection({planId:"plan",planVersion:1,windowId:"window",windowVersion:1,dependentIds:["dependent"]},"key");
  expect(f.mutations[0]).toMatchObject({action:"election",learning:"learning-receipt",person:{id:"worker"},now:Date.UTC(2026,9,5)});
  expect(f.calls.slice(-2)).toEqual(["prepare-learning","commit"]);
 });
 test("rejects duplicate dependents and an expired window before staging effects",async()=>{
  const duplicate=benefitsFixture();
  await expect(duplicate.app.createElection({planId:"plan",planVersion:1,windowId:"window",windowVersion:1,dependentIds:["dependent","dependent"]},"key")).rejects.toMatchObject({code:"VERIFIED_DEPENDENT_REQUIRED"});
  const closed=benefitsFixture({async window(){return {opens_on:"2026-09-01",closes_on:"2026-09-30"};}});
  await expect(closed.app.createElection({planId:"plan",planVersion:1,windowId:"window",windowVersion:1,dependentIds:["dependent"]},"key")).rejects.toMatchObject({code:"ENROLLMENT_WINDOW_CLOSED"});
  expect([...duplicate.mutations,...closed.mutations]).toEqual([]);
 });
 test("retains permission checks even for a transition receipt replay",async()=>{
  const f=benefitsFixture({async replay(){return {id:"saved"};},async authorizeReview(){throw new Error("revoked");}});
  await expect(f.app.change("plan","plan",{action:"publish",expectedVersion:1,reason:"Review coverage"},"key")).rejects.toThrow("revoked");
 });
 test("stages evidence and retained Learning validation for provider acknowledgement",async()=>{
  const f=benefitsFixture();
  await f.app.change("election","election",{action:"acknowledge",expectedVersion:2,reason:"Provider accepted coverage",providerReference:"provider-42",attachmentSetIds:["set"]},"key");
  expect(f.mutations[0]).toMatchObject({action:"transition",from:"submitted",to:"acknowledged",evidence:"file-binding",learning:"learning-guards"});
  expect(f.calls.slice(-3)).toEqual(["evidence","validate-learning","commit"]);
 });
 test("independent payroll release and personal visibility use owner facts",async()=>{
  const f=benefitsFixture({async record(){return {status:"acknowledged",version:3,acknowledged_by:"reviewer"};}});
  await expect(f.app.change("election","election",{action:"release",expectedVersion:3,reason:"Release payroll deduction"},"key")).rejects.toMatchObject({code:"CONFLICT"});
  const personal=benefitsFixture({async elections(personId,reviewer){expect(personId).toBe("worker");expect(reviewer).toBe(false);return [];}});
  expect((await personal.app.workspace(true)).elections).toEqual([]);
 });
});
