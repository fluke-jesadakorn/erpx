import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import { env, applyMigrations } from "../../support/runtime-env";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { GET as legacyListGET } from "../../../src/app/api/v1/holdings/[holdingSlug]/people/route";
import { GET as legacyDetailGET } from "../../../src/app/api/v1/holdings/[holdingSlug]/people/[...path]/route";
import { GET as publicDirectoryGET } from "../../../src/app/api/v1/holdings/[holdingSlug]/people-public-directory/route";

const state = vi.hoisted(() => ({ actor: null as WorkContext | null }));
vi.mock("../../../src/server/modules/identity_access/public/authorization", () => ({
  requireWorkContext: async (slug: string) => {
    if (slug !== "test" || !state.actor) throw new Error("FORBIDDEN");
    return state.actor;
  },
}));
vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({
  holdingBindingBySlug: async (slug: string) => {
    if (slug !== "test") throw new Error("HOLDING_NOT_FOUND");
    return { id: "h001", slug, displayName: "Test Holding" };
  },
  holdingResources: async (holdingId: string) => {
    if (holdingId !== "h001") throw new Error("HOLDING_NOT_FOUND");
    return { write: env.H001_DB, files: env.H001_FILES };
  },
  runtimeEnv: async () => env,
}));

let fixture: Awaited<ReturnType<typeof seedExpenseCompany>>;
beforeAll(async () => { await applyMigrations(env.H001_DB, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => {
  fixture = await seedExpenseCompany(env.H001_DB, env.H001_FILES);
  state.actor = fixture.claimant;
});

const request = (path: string) => new Request(`http://localhost:3100/api/v1/holdings/test/${path}`);
const listContext = { params: Promise.resolve({ holdingSlug: "test" }) };
const detailContext = (path: string[]) => ({ params: Promise.resolve({ holdingSlug: "test", path }) });

async function seedPrivateProfile() {
  const personId = `person-${fixture.head.userId}`;
  const engagementId = `eng-${fixture.head.userId}`;
  await env.H001_DB.prepare("UPDATE hr_people SET email=?,phone=?,work_email=?,work_phone=? WHERE id=?")
    .bind("private@example.test", "private-number", "work@example.test", "+66 2 123 4567", personId).run();
  await env.H001_DB.prepare(`INSERT INTO hr_compensation_revisions
    (id,engagement_id,effective_from,pay_frequency,base_amount_minor,currency,status,created_by,created_at,updated_at)
    VALUES(?,?,'2020-01-01','monthly',912345,'THB','approved','fixture',1,1)`)
    .bind(`comp-${crypto.randomUUID()}`, engagementId).run();
  return personId;
}

it("denies ordinary readers legacy People data while keeping the public Holding directory available", async () => {
  const otherPersonId = await seedPrivateProfile();
  const legacyList = await legacyListGET(request("people"), listContext);
  expect(legacyList.status).toBe(403);
  expect((await legacyList.json()).error.code).toBe("PERMISSION_REQUIRED");

  const otherDetail = await legacyDetailGET(request(`people/people/${otherPersonId}`),
    detailContext(["people", otherPersonId]));
  expect(otherDetail.status).toBe(403);
  expect((await otherDetail.json()).error.code).toBe("PERSON_DETAILS_FORBIDDEN");

  const organization = await legacyDetailGET(request("people/organization"),
    detailContext(["organization"]));
  expect(organization.status).toBe(403);
  expect((await organization.json()).error.code).toBe("PERMISSION_REQUIRED");

  const publicDirectory = await publicDirectoryGET(request("people-public-directory"),
    { params: Promise.resolve({ holdingSlug: "test" }) });
  expect(publicDirectory.status).toBe(200);
  expect(publicDirectory.headers.get("Cache-Control")).toBe("private, no-store");
  const payload = await publicDirectory.json();
  expect(payload.data.people.find((person: { personId: string }) => person.personId === otherPersonId))
    .toMatchObject({ workEmail: "work@example.test", workPhone: "+66 2 123 4567" });
  expect(JSON.stringify(payload)).not.toContain("private@example.test");
  expect(JSON.stringify(payload)).not.toContain("private-number");
  expect(JSON.stringify(payload)).not.toContain("912345");

  const ownPersonId = `person-${fixture.claimant.userId}`;
  await env.H001_DB.prepare("UPDATE hr_people SET email=?,phone=? WHERE id=?")
    .bind("own-private@example.test", "own-private-number", ownPersonId).run();
  const ownDetail = await legacyDetailGET(request(`people/people/${ownPersonId}`),
    detailContext(["people", ownPersonId]));
  expect(ownDetail.status).toBe(200);
  expect((await ownDetail.json()).data).toMatchObject({
    privateAccess: true,
    person: { id: ownPersonId, email: "own-private@example.test", phone: "own-private-number" },
  });

  state.actor = fixture.owner;
  const hrList = await legacyListGET(request("people"), listContext);
  expect(hrList.status).toBe(200);
  expect((await hrList.json()).data.find((person: { id: string }) => person.id === otherPersonId))
    .toMatchObject({ email: "private@example.test" });
  const hrOrganization = await legacyDetailGET(request("people/organization"),
    detailContext(["organization"]));
  expect(hrOrganization.status).toBe(200);
});

it("lets compensation staff read pay but strips another person's private profile fields", async () => {
  const otherPersonId = await seedPrivateProfile();
  state.actor = await fixture.member("payroll-reader", "payroll_preparer");
  const list = await legacyListGET(request("people"), listContext);
  expect(list.status).toBe(403);

  const detail = await legacyDetailGET(request(`people/people/${otherPersonId}`),
    detailContext(["people", otherPersonId]));
  expect(detail.status).toBe(200);
  const payload = await detail.json();
  expect(payload.data.privateAccess).toBe(true);
  expect(payload.data.revisions[0].base_amount_minor).toBe(912345);
  expect(Object.keys(payload.data.person).sort()).toEqual([
    "currency", "engagement_id", "engagement_version", "id", "pay_group_id",
  ].sort());
  expect(JSON.stringify(payload)).not.toContain("private@example.test");
  expect(JSON.stringify(payload)).not.toContain("private-number");

  const organization = await legacyDetailGET(request("people/organization"),
    detailContext(["organization"]));
  expect(organization.status).toBe(403);
});

it("checks explorer tabs on the server and exposes only work contacts to ordinary readers", async () => {
 const { GET } = await import("../../../src/app/api/v1/holdings/[holdingSlug]/people/explorer-detail/route");
 const personId=await seedPrivateProfile(),id=`eng-${fixture.head.userId}`;
 const detail=(tab:string,selected=id)=>GET(request(`people/explorer-detail?kind=person&id=${selected}&tab=${tab}`),listContext);
 const profile=await detail('Profile');expect(profile.status).toBe(200);
 const publicPayload=await profile.json();expect(publicPayload.data.privatePerson).toBeNull();
 expect(JSON.stringify(publicPayload)).not.toContain('private@example.test');expect(JSON.stringify(publicPayload)).not.toContain('912345');
 for(const tab of ['Employment','History','Compensation'])expect((await detail(tab)).status).toBe(403);
 expect((await detail('Profile','unavailable-engagement')).status).toBe(404);
 state.actor=fixture.owner;
 const hr=await detail('Profile');expect(hr.status).toBe(200);const payload=await hr.json();
 expect(payload.data.privatePerson.person).toMatchObject({id:personId,engagement_id:id,email:'private@example.test'});
 expect(payload.data.privatePerson.revisions).toEqual([]);expect(JSON.stringify(payload)).not.toContain('912345');
 const compensation=await detail('Compensation');expect(compensation.status).toBe(200);
 expect((await compensation.json()).data.privatePerson.revisions[0].base_amount_minor).toBe(912345);
});

it("loads exactly the engagement selected in an explorer panel",async()=>{
 const { GET } = await import("../../../src/app/api/v1/holdings/[holdingSlug]/people/explorer-detail/route");
 const personId=`person-${fixture.head.userId}`,first=`eng-${fixture.head.userId}`,second=`eng-second-${crypto.randomUUID()}`;
 await env.H001_DB.prepare(`INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,job_title,worker_type,employment_type,start_date,status,currency,created_by,created_at,updated_at) SELECT ?,person_id,company_id,branch_id,department_id,'Second engagement',worker_type,employment_type,'2026-01-01',status,currency,created_by,created_at,updated_at FROM hr_engagements WHERE id=?`).bind(second,first).run();
 state.actor=fixture.owner;
 for(const id of [first,second]){
  const response=await GET(request(`people/explorer-detail?kind=person&id=${id}&tab=Employment`),listContext);expect(response.status).toBe(200);
  expect((await response.json()).data.privatePerson.person).toMatchObject({id:personId,engagement_id:id});
 }
});
