import { describe, expect, test } from "vitest";
import { buildProjectGraph, type Actual, type ProjectGraphSource } from "@erpx/server-projects/domain/graph";
import { createProjectGraphApplication } from "@erpx/server-projects/application/graph";
import { evaluateQualityMeasurements } from "@erpx/server-quality/domain/measurements";
import { createSalesPricingApplication } from "@erpx/server-sales/application/pricing";
import { milliQuantity } from "@erpx/server-inventory/domain/reorder-quantity";
import { exactConversion } from "@erpx/server-procurement/domain/quantity-conversion";

function graphSource(): ProjectGraphSource {
  const actual = (id: string, data: Partial<Actual>): Actual => ({ id, milestone_id: "child", source_kind: "work_log", source_id: id, source_version: 1, current_source_version: 1, source_state: "approved", minutes: 30, amount_minor: null, currency: null, user_id: "person-1", journal_status: null, journal_id: null, ...data });
  return {
    project: { id: "project", owner_user_id: "owner" } as ProjectGraphSource["project"],
    milestones: [
      { id: "parent", parent_milestone_id: null, ordinal_position: 0, depends_on_ids: [] },
      { id: "child", parent_milestone_id: "parent", ordinal_position: 0, depends_on_ids: [] },
    ] as unknown as ProjectGraphSource["milestones"],
    tasks: [{ id: "task", title: "Task", state: "assigned", version: 1, assignee_user_id: "person-1", milestone_id: "child", binding_version: 1 }],
    actuals: [actual("own-time", {}), actual("other-time", { user_id: "person-2" }), actual("stale-time", { source_version: 1, current_source_version: 2 }), actual("cost", { source_kind: "cost_allocation", minutes: null, amount_minor: 15000, currency: "THB", user_id: null, journal_status: "posted" }), actual("unposted-cost", { source_kind: "cost_allocation", minutes: null, amount_minor: 9000, currency: "THB", user_id: null, journal_status: "draft" })],
  };
}

describe("Projects owner graph projection", () => {
  test("filters person time and Finance costs, builds WBS and counts only current approved sources", () => {
    const source = graphSource();
    const personal = buildProjectGraph(source, { isOwner: false, canReadCosts: false, isSamePerson: (id) => id === "person-1" });
    expect(personal.actuals.map((actual) => actual.id)).toEqual(["own-time", "stale-time"]);
    expect(personal.totals).toEqual({ approvedMinutes: 30, costByCurrency: {} });
    expect(personal.packages.map((entry) => entry.wbs)).toEqual(["1", "1.1"]);
    const owner = buildProjectGraph(source, { isOwner: true, canReadCosts: true, isSamePerson: () => false });
    expect(owner.totals).toEqual({ approvedMinutes: 60, costByCurrency: { THB: "15000" } });
    expect(owner.edges).toEqual([{ from: "parent", to: "child", kind: "contains" }, { from: "child", to: "task", kind: "task" }]);
  });
  test("rejects disconnected/cyclic work packages and loads through the supplied repository", async () => {
    const source = graphSource();
    const app = createProjectGraphApplication({ async read(id) { expect(id).toBe("project"); return source; }, viewer: () => ({ isOwner: true, canReadCosts: false, isSamePerson: () => false }) });
    expect((await app("project")).totals.approvedMinutes).toBe(60);
    source.milestones[0].parent_milestone_id = "child";
    expect(() => buildProjectGraph(source, { isOwner: true, canReadCosts: true, isSamePerson: () => false })).toThrow("The project structure requires recovery");
  });
});

describe("Quality measurement decisions", () => {
  const metrics = [{ id: "width", minimum_value: "10", maximum_value: "12" }, { id: "height", minimum_value: null, maximum_value: "20" }];
  test("derives pass/fail from inclusive limits and rejects explicit contradictory results", () => {
    expect(evaluateQualityMeasurements(metrics, [{ metricId: "width", value: "10" }, { metricId: "height", value: "20" }]).result).toBe("pass");
    expect(evaluateQualityMeasurements(metrics, [{ metricId: "width", value: "9.99" }, { metricId: "height", value: "20" }]).result).toBe("fail");
    expect(() => evaluateQualityMeasurements(metrics, [{ metricId: "width", value: "10" }, { metricId: "height", value: "20" }], "fail")).toThrow("calculated from the recorded limits");
  });
  test("requires exactly one metric value, a finite value and explicit result without a specification", () => {
    expect(() => evaluateQualityMeasurements(metrics, [{ metricId: "width", value: "10" }, { metricId: "width", value: "10" }])).toThrow("one value for every measurement");
    expect(evaluateQualityMeasurements(metrics, [{ metricId: "width", value: "Infinity" }, { metricId: "height", value: "20" }]).result).toBe("fail");
    expect(() => evaluateQualityMeasurements([], [])).toThrow("Choose pass or fail");
    expect(evaluateQualityMeasurements([], [], "pass").result).toBe("pass");
  });
});

describe("Sales approved-price selection", () => {
  test("purchase bypasses sales price-list reads; a selected unavailable list is rejected", async () => {
    const base = { partyId: "customer", orderDate: "2026-10-05", lines: [{ productId: "product", price: "90.00", taxBps: 700 }] };
    const app = createSalesPricingApplication({ async list() { return null; }, async items() { throw new Error("must not load absent list"); } });
    expect((await app({ ...base, kind: "purchase" })).requiresApproval).toBe(false);
    await expect(app({ ...base, kind: "sales", priceListId: "unavailable" })).rejects.toMatchObject({ code: "PRICE_LIST_INVALID" });
  });
  test("requires discount approval/reason beyond the approved threshold and preserves tax treatment", async () => {
    const base = { kind: "sales" as const, partyId: "customer", orderDate: "2026-10-05", lines: [{ productId: "product", price: "90.00", taxBps: 700 }] };
    const app = createSalesPricingApplication({ async list() { return { id: "list", max_auto_discount_bps: 500 }; }, async items() { return [{ product_id: "product", price_minor: 10000, tax_bps: 700 }]; } });
    await expect(app(base)).rejects.toMatchObject({ code: "DISCOUNT_REASON_REQUIRED" });
    expect((await app({ ...base, discountReason: "Approved customer discount" })).requiresApproval).toBe(true);
    await expect(app({ ...base, lines: [{ ...base.lines[0], taxBps: 0 }] })).rejects.toMatchObject({ code: "PRICE_LIST_TAX_REQUIRED" });
  });
});

describe("Inventory and Procurement exact quantities", () => {
  test("retains precise milli-quantities within PostgreSQL bigint limits", () => {
    expect(milliQuantity("1.001")).toBe("1001");
    expect(milliQuantity("9223372036854775.807")).toBe("9223372036854775807");
    expect(() => milliQuantity("9223372036854775.808")).toThrow("positive inventory quantity");
    expect(() => milliQuantity("0")).toThrow();
  });
  test("detects supplier-unit conversion drift without floating-point multiplication", () => {
    expect(exactConversion(1001, 1001, 1000)).toBe(true);
    expect(exactConversion(1001, 1000, 1000)).toBe(false);
    expect(exactConversion(2500, 5000, 500)).toBe(true);
  });
});
import { createCRMApplication, type CRMRepository, type CRMMutation } from "@erpx/server-crm/application/leads";
import { createCRMActivityApplication, type CRMActivityPorts, type CRMActivityMutation } from "@erpx/server-crm/application/activities";
import { createCRMIdentityApplication, type CRMIdentityPorts, type CRMIdentityMutation } from "@erpx/server-crm/application/identities";
import { analyticsRange, parseAnalyticsFilters } from "@erpx/server-crm/domain/analytics";
import type { CRMActivity } from "@erpx/server-crm/domain/activities";

function crmFixture(overrides:Partial<Parameters<typeof createCRMApplication>[0]>={},repositoryOverrides:Partial<CRMRepository>={}){
 const mutations:CRMMutation[]=[],calls:string[]=[];
 const repository:CRMRepository={
  async records(){calls.push("records");return [{id:"lead",owner_id:"salesperson",status:"open",branch_id:"branch",version:1}];},
  async branches(){return [];},async campaigns(){return [];},async evidence(){return null;},async recipients(){return [];},async customers(){return [];},async quotes(){return [];},async sales(){return [];},async currency(){return {currency:"THB"};},async guestCandidates(){throw new Error("PMS must require permission");},async reservations(){throw new Error("PMS must require permission");},async sourceOrders(){return [];},async sourceSupport(){throw new Error("Support must require permission");},async sourceFollowups(){return [];},async history(){return [];},
  async replay(){calls.push("replay");return null;},async commit(mutation,result){mutations.push(mutation);return result;},...repositoryOverrides,
 };
 const app=createCRMApplication({actorId:"salesperson",canRead:true,canSales:true,canApprove:false,canPms:false,canSupport:false,clock:{now:()=>42},authorize(permission){calls.push(permission);},async repository(){return repository;},async activityWork(){return [];},...overrides});
 return {app,mutations,calls};
}
describe("CRM owner application",()=>{
 test("checks a saved create receipt before evaluating a contact's changed account prerequisite",async()=>{
  const saved={id:"saved",version:1};const f=crmFixture({}, {async replay<Result>(){return saved as Result;}});
  expect(await f.app.create({branchId:"branch",title:"Customer inquiry",contactName:"Customer",contactDetail:"Contact details",source:"Staff inquiry",contactId:"contact",brief:"Discuss services",dueDate:"2026-10-06"},"key")).toEqual(saved);
  expect(f.mutations).toEqual([]);expect(f.calls).toEqual(["commerce.sales","commerce.read"]);
 });
 test("qualifies an owned lead with Company currency through the semantic repository",async()=>{
  const f=crmFixture();
  await f.app.changeLead("lead",{action:"qualify",expectedVersion:1,customerId:"customer",expectedValue:"1200.50",closeDate:"2026-10-31",reason:"Customer needs are confirmed"},"key");
  expect(f.mutations[0]).toMatchObject({action:"lead",opportunityId:"opportunity-key",currency:"THB",now:42});
 });
 test("requires a sales manager to reopen and forbids another salesperson from qualifying",async()=>{
  const reopen=crmFixture();
  await expect(reopen.app.changeLead("lead",{action:"reopen",expectedVersion:1,reason:"Reconsider customer needs"},"key")).rejects.toMatchObject({code:"CRM_MANAGER_REQUIRED"});
  const other=crmFixture({actorId:"other"});
  await expect(other.app.changeLead("lead",{action:"qualify",expectedVersion:1,customerId:"customer",expectedValue:"100",closeDate:"2026-10-31",reason:"Qualify customer needs"},"key")).rejects.toMatchObject({code:"CRM_OWNER_REQUIRED"});
 });
 test("requires an approved quotation before negotiating and does not commit a rejected command",async()=>{
  const f=crmFixture({}, {async records(){return [{id:"lead",owner_id:"salesperson",status:"qualified",opportunity_id:"opportunity",stage:"discovery"}];}});
  await expect(f.app.changeOpportunity("lead",{action:"negotiate",expectedVersion:1,leadVersion:2,reason:"Negotiate reviewed terms"},"key")).rejects.toMatchObject({code:"CRM_QUOTATION_REQUIRED"});
  expect(f.mutations).toEqual([]);
 });
 test("keeps foreign provider projections behind permission facts",async()=>{
  const f=crmFixture({}, {async records(){return [{id:"lead",customer_id:"customer",contact_id:"contact",account_customer_id:"customer",owner_id:"salesperson",owner_eligible:1}];}});
  expect(await f.app.detail("lead")).toMatchObject({guestCandidates:[],reservations:[],sourceSupport:[],canAct:true,canManage:false});
 });
});
describe("CRM activity and identity ports",()=>{
 test("activity creation replays before reading a now-closed lead",async()=>{
  const ports:CRMActivityPorts={actorId:"owner",hasRead:true,sales:true,manager:false,clock:{now:()=>42},isSamePerson:()=>false,authorize(){},async replay(){return {id:"saved"};},async lead(){throw new Error("must not read");},async activities(){return [];},async history(){return [];},async recipients(){return [];},async activity(){return null;},async workRows(){return [];},async commit(){throw new Error("must not commit");}};
  expect(await createCRMActivityApplication(ports).create({leadId:"lead",expectedVersion:1,activityType:"call",subject:"Call customer",details:"Discuss delivery",ownerId:"owner",dueDate:"2026-10-06"},"key")).toEqual({id:"saved"});
 });
 test("activity completion requires the assigned owner even when the creator is the same person",async()=>{
  const mutations:CRMActivityMutation[]=[];
  const ports:CRMActivityPorts={actorId:"creator",hasRead:true,sales:true,manager:false,clock:{now:()=>42},isSamePerson:id=>id==="creator",authorize(){},async replay(){return null;},async lead(){return null;},async activities(){return [];},async history(){return [];},async recipients(){return [];},async activity(){return {owner_id:"assigned-owner",created_by:"creator",status:"open"} as CRMActivity;},async workRows(){return [];},async commit(m,result){mutations.push(m);return result;}};
  await expect(createCRMActivityApplication(ports).change("activity",{action:"complete",expectedVersion:1,reason:"Customer answered the call"},"key")).rejects.toMatchObject({code:"CRM_ACTIVITY_OWNER_REQUIRED"});
  expect(mutations).toEqual([]);
 });
 test("identity linking preserves independent review requirements as a commit contract and validates access after replay",async()=>{
  const mutations:CRMIdentityMutation[]=[],calls:string[]=[];
  const ports:CRMIdentityPorts={canReview:true,canCreate:true,clock:{now:()=>42},authorize(permission){calls.push(permission);},async replay(){calls.push("replay");return null;},async identities(){return {accounts:[],contacts:[],customers:[]};},async contact(){return null;},async stays(){return [];},async commit(m,result){mutations.push(m);return result;}};
  await createCRMIdentityApplication(ports).change({action:"link-finance",accountId:"account",customerId:"customer",expectedVersion:2,evidence:"Independently match the customer"},"key");
  expect(calls).toEqual(["commerce.sales","replay","commerce.approve"]);
  expect(mutations[0]).toMatchObject({recordId:"account",operation:"crm.identity.link-finance",version:3,now:42});
 });
 test("analytics range and Branch scope are deterministic through passed facts",()=>{
  expect(analyticsRange(Date.UTC(2026,9,5,20))).toEqual({from:"2026-09-06",to:"2026-10-06"});
  expect(()=>parseAnalyticsFilters({branchId:"branch"},{from:"2026-10-01",to:"2026-10-31",branchId:"other"})).toThrow("Branch is outside your workspace.");
 });
});
