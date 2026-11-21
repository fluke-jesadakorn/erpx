import { beforeAll, expect, it, vi } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { peopleRelationshipsSnapshot, peopleAccessSnapshot } from "@erpx/server-identity_access/public/people-access";
import { loadIamAuthority } from "@erpx/server-identity_access/public/iam-store";
import { holdingPeopleDirectory } from "@erpx/server-organization/public/holding-people-directory";
import { directoryPerson } from "@erpx/server-people_payroll/public/people-directory";
import { peopleDirectoryPage } from "@erpx/server-people_payroll/public/people-directory-page";
import type { IamActor } from "@erpx/server-identity_access/public/iam-context";
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({runtimeEnv:async()=>env,holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),holdingBindingBySlug:async()=>({id:"h001",slug:"test",displayName:"Test"})}));
const db=env.H001_DB,key=()=>crypto.randomUUID();
beforeAll(async()=>{await applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS);await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
/**
 * Reads only. Privilege and relationship commands are native now, so this
 * fixture seeds the reviewed state directly and asserts the projection.
 */
async function fixture(){
  const f=await seedExpenseCompany(db,env.H001_FILES);
  async function actor(role:string){
    const work=await f.member(role,role),sessionId=key(),now=Date.now();
    await env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,1,?,?)').bind(work.userId,role,`${work.userId}@example.invalid`,now,now).run();
    await env.CONTROL_DB.prepare('INSERT INTO session(id,"expiresAt",token,"createdAt","updatedAt","userId") VALUES(?,?,?,?,?,?)').bind(sessionId,new Date(now+86400000).toISOString(),key(),new Date(now).toISOString(),new Date(now).toISOString(),work.userId).run();
    await env.CONTROL_DB.prepare("INSERT INTO iam_mfa(user_id,secret,enabled) VALUES(?,'fixture-only',true)").bind(work.userId).run();
    await env.CONTROL_DB.prepare("INSERT INTO iam_mfa_sessions VALUES(?,?,?)").bind(sessionId,work.userId,now).run();
    return {...work,sessionId,scopeKind:"holding",scopeNodeId:null,platform:false,authority:{sessionId,revision:1,assignmentVersion:1},iam:await loadIamAuthority(db,f.company,work.userId,role)} satisfies IamActor;
  }
  const it=await actor("it_admin"),hr=await actor("hr_admin");
  const organization=`iam-organization-${key().slice(0,20)}`,team=`iam-team-${key().slice(0,20)}`,subject=`iam-subject-${key().slice(0,20)}`,role=`custom_${key().slice(0,20)}`,now=Date.now();
  await db.batch([
    db.prepare("INSERT INTO iam_external_organizations(id,company_id,name,kind,version) VALUES(?,?,?,'organization',1)").bind(organization,f.company,"Vendor company"),
    db.prepare("INSERT INTO iam_external_teams(id,company_id,organization_id,name,department_id,version) VALUES(?,?,?,?,?,1)").bind(team,f.company,organization,"Vendor support",f.department),
    db.prepare("INSERT INTO iam_access_subjects(id,company_id,kind,name,email,organization_id,team_id,sponsor_person_id,expires_at,status,version) VALUES(?,?,'external',?,?,?,?,?,?,'active',1)").bind(subject,f.company,"External technician",`${key()}@example.invalid`,organization,team,`person-${f.owner.userId}`,now+2*86400000),
    db.prepare("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision) VALUES(?,?,?,false,1)").bind(f.company,role,"Vendor read access"),
    db.prepare("INSERT INTO iam_role_revisions VALUES(?,?,1,?::jsonb,'[]'::jsonb,'[\"department\"]','fixture',1)").bind(f.company,role,JSON.stringify(["module.collaboration.read","iam.self"])),
  ]);
  return {...f,it,hr,organization,team,subject,role};
}
it("keeps external organizations and teams out of the Company tree and HR directory",async()=>{
  const f=await fixture();
  await db.prepare("UPDATE hr_engagements SET worker_type='contractor',employment_type='contractor' WHERE person_id=?").bind(`person-${f.claimant.userId}`).run();
  const directory=await holdingPeopleDirectory(f.hr);
  expect(directory.people.some(p=>p.personId===`person-${f.claimant.userId}`)).toBe(false);
  expect(JSON.stringify(directory)).not.toContain("External technician");
  expect(await db.prepare("SELECT id FROM organization_nodes WHERE id IN (?,?)").bind(f.organization,f.team).first()).toBeNull();
  await expect(directoryPerson(f.owner,`person-${f.claimant.userId}`)).rejects.toMatchObject({code:"PERSON_NOT_FOUND"});
  await expect(peopleAccessSnapshot(db,f.hr)).rejects.toMatchObject({status:403});
  const page = await peopleDirectoryPage(f.hr, {q:"External technician", person:f.subject});
  expect(page.items).toEqual([]); expect(page.selected).toBeNull();
  const legacy = await peopleDirectoryPage(f.hr, {person:`person-${f.claimant.userId}`});
  expect(legacy.selected).toBeNull();
  const access=await peopleAccessSnapshot(db,f.it);
  expect(access.subjects.find(s=>s.id===f.subject)).toMatchObject({kind:"external",personId:null});
  expect(access.teams.find(t=>t.id===f.team)?.departmentId).toBe(f.department);
  expect(access.assignments.some(a=>a.userId===access.subjects.find(s=>s.id===f.subject)?.userId)).toBe(false);
});
it("HR reads only Company relationships, without grants or Directory configuration",async()=>{
  const f=await fixture();
  const data=await peopleRelationshipsSnapshot(db,f.hr);
  expect(data.canManage).toBe(true);
  for(const key of ["roles","permissions","assignments","connections","authorizingUntil"])expect(data).not.toHaveProperty(key);
  expect((data as unknown as {subjects:{id:string}[]}).subjects.some(s=>s.id===f.subject)).toBe(true);
  const access=await peopleAccessSnapshot(db,f.it);
  expect(access.canApply).toBe(true);
  expect(access.roles.some(r=>r.id===f.role)).toBe(true);
  await expect(peopleRelationshipsSnapshot(db,{...f.hr,scopeKind:"branch",branchId:f.branch,scopeNodeId:f.branch})).rejects.toMatchObject({code:"COMPANY_SCOPE_REQUIRED"});
});
it("classifies one verified account independently in two Companies without colliding relationship IDs",async()=>{
  const first=await fixture(),second=await fixture(),userId=key(),now=Date.now();
  await env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,1,?,?)').bind(userId,"Shared verified account",`${userId}@example.invalid`,now,now).run();
  for(const f of [first,second])await db.prepare("INSERT INTO iam_members(company_id,user_id,principal_id) VALUES(?,?,?)").bind(f.company,userId,key()).run();
  const a=(await peopleRelationshipsSnapshot(db,first.hr)).subjects.find(s=>s.userId===userId)!,b=(await peopleRelationshipsSnapshot(db,second.hr)).subjects.find(s=>s.userId===userId)!;
  expect(a.classification).toBe("unclassified");expect(b.classification).toBe("unclassified");expect(a.id).not.toBe(b.id);
  expect((await db.prepare("SELECT count(*) count FROM assignments WHERE user_id=?").bind(userId).first<{count:number}>())?.count).toBe(0);
});
it("keeps an external relationship inactive until the invited alias is verified",async()=>{
  const f=await fixture();
  const subject=(await peopleRelationshipsSnapshot(db,f.hr)).subjects.find(s=>s.id===f.subject)!;
  expect(subject).toMatchObject({kind:"external",status:"active"});
  expect(subject.accountSource).toBeNull();
  const alias=key();
  await db.prepare("UPDATE iam_access_subjects SET person_id=? WHERE id=?").bind(f.subject,f.subject).run();
  await db.prepare("INSERT INTO iam_members(company_id,user_id,principal_id,person_id) VALUES(?,?,?,?)").bind(f.company,alias,key(),f.subject).run();
  const linked=(await peopleRelationshipsSnapshot(db,f.hr)).subjects.find(s=>s.id===f.subject)!;
  expect(linked.linkedUserIds).toEqual(expect.arrayContaining([alias]));
  expect((await db.prepare("SELECT iam_principal_active(?,?,?) ok").bind(alias,f.company,Date.now()).first<{ok:boolean}>())?.ok).toBe(true);
  await db.prepare("UPDATE iam_access_subjects SET expires_at=? WHERE id=?").bind(Date.now()-1,f.subject).run();
  expect((await db.prepare("SELECT iam_principal_active(?,?,?) ok").bind(alias,f.company,Date.now()).first<{ok:boolean}>())?.ok).toBe(false);
});
