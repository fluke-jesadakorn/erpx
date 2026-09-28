import { beforeAll, expect, it, vi } from "vitest";
import { env, applyMigrations } from "./runtime-env";
import { expenseFixture } from "./helpers/expense-fixture";
import { recommendationStatements, defaultCatalog, savePositionPayDefault, saveLevelLeaveDefault } from "../src/server/staff-defaults";
import { transitionCompensation } from "../src/server/people-payroll";
import type { WorkContext } from "../src/server/authorization";

vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
const db=env.H001_DB;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);await applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS);});

it("keeps pay private, creates an independently approved draft, preserves approved pay, and dates a changed leave level next January",async()=>{
  const fixture=await expenseFixture(false),company=fixture.company,owner=fixture.owner;
  const engagement=await db.prepare("SELECT e.id,e.level_id,e.currency FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE p.user_id=? AND e.company_id=? LIMIT 1").bind(fixture.claimant.userId,company).first<{id:string;level_id:string;currency:string}>();
  expect(engagement).toBeTruthy();
  const group=`group-${company}`,position=`position-${company}`,level=`numbered-${company}`,policy=`default-policy-${company}`;
  await db.batch([
    db.prepare("INSERT INTO staff_levels(id,company_id,code,name,rank,level_number,active,created_at,updated_at) VALUES(?,?,?,'Level 6',6,6,1,1,1)").bind(level,company,level),
    db.prepare("INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,status,created_by,created_at,updated_at) VALUES(?,?,?,'TH',?,'monthly','active',?,1,1)").bind(group,company,group,engagement!.currency,owner.userId),
  ]);
  await db.prepare("INSERT INTO organization_positions(id,company_id,title,level_id,is_ceo,created_by,created_at,updated_at) VALUES(?,?,?, ?,1,?,1,1)").bind(position,company,"Fixture position",level,owner.userId).run();
  await db.prepare("UPDATE hr_engagements SET level_id=?,position_id=? WHERE id=?").bind(level,position,engagement!.id).run();
  await db.prepare("INSERT INTO position_pay_defaults(id,company_id,position_id,version,pay_group_id,currency,monthly_amount_minor,created_by,created_at) VALUES(?,?,?,1,?,?,360000,?,1)").bind(`pay-${company}`,company,position,group,engagement!.currency,owner.userId).run();
  for(const code of ["annual","sick"])await db.prepare("INSERT INTO leave_types(id,company_id,code,name,tracked,effective_from,created_by,created_at,updated_at) VALUES(?,?,?,?,true,'2026-01-01',?,1,1)").bind(`${code}-${company}`,company,code,code,owner.userId).run();
  await db.prepare("INSERT INTO leave_policy_versions(id,company_id,policy_key,version,name,status,effective_from,created_by,created_at) VALUES(?,?,?,1,'Level 6','active','2026-01-01',?,1)").bind(policy,company,"level-6-default",owner.userId).run();
  for(const [code,days] of [["annual",6],["sick",15]] as const)await db.prepare("INSERT INTO leave_policy_rules(policy_version_id,leave_type_id,grant_kind,grant_units,accrual_units) VALUES(?,?,'annual',?,0)").bind(policy,`${code}-${company}`,days).run();
  await db.prepare("INSERT INTO level_leave_defaults(id,company_id,level_id,version,policy_version_id,created_by,created_at) VALUES(?,?,?,1,?,?,1)").bind(`leave-${company}`,company,level,policy,owner.userId).run();
  const reader={...owner,role:"people_manager",iam:undefined} as WorkContext;
  expect((await defaultCatalog(db,reader)).pay).toEqual([]);
  expect((await defaultCatalog(db,owner)).pay[0].monthly_amount_minor).toBe(360000);
  const statements=await recommendationStatements(db,owner,{engagementId:engagement!.id,positionId:position,levelId:level,priorLevelId:null,startDate:"2026-09-01",currency:engagement!.currency,acceptPay:true,acceptLeave:true,commandId:`new-${company}`});
  await db.batch(statements);
  const draft=await db.prepare("SELECT id,status,version FROM hr_compensation_revisions WHERE engagement_id=?").bind(engagement!.id).first<{id:string;status:string;version:number}>();
  expect(draft?.status).toBe("draft");
  await expect(transitionCompensation(owner,draft!.id,{status:"approved",expectedVersion:1},`self-${company}`)).rejects.toMatchObject({code:"SELF_REVIEW_FORBIDDEN"});
  await db.prepare("UPDATE hr_compensation_revisions SET created_by=? WHERE id=?").bind(fixture.claimant.userId,draft!.id).run();
  await transitionCompensation(owner,draft!.id,{status:"approved",expectedVersion:1},`approve-${company}`);
  await db.batch(await recommendationStatements(db,owner,{engagementId:engagement!.id,positionId:position,levelId:level,priorLevelId:null,startDate:"2026-09-01",currency:engagement!.currency,acceptPay:true,acceptLeave:false,commandId:`repeat-${company}`}));
  expect((await db.prepare("SELECT count(*) n FROM hr_compensation_revisions WHERE engagement_id=?").bind(engagement!.id).first<{n:number}>())?.n).toBe(1);
  const changed=await recommendationStatements(db,owner,{engagementId:engagement!.id,positionId:position,levelId:level,priorLevelId:engagement!.level_id,startDate:"2026-09-01",currency:engagement!.currency,acceptPay:false,acceptLeave:true,commandId:`change-${company}`});
  await db.batch(changed);
  const assignments=(await db.prepare("SELECT effective_from,effective_to FROM leave_policy_assignments WHERE engagement_id=? ORDER BY effective_from").bind(engagement!.id).all<{effective_from:string;effective_to:string|null}>()).results;
  const year=new Date().getUTCFullYear();
  expect(assignments.at(-1)?.effective_from).toBe(`${year+1}-01-01`);
  expect(assignments.at(-2)?.effective_to).toBe(`${year}-12-31`);
  const futureYear=year+2;
  await db.batch(await recommendationStatements(db,owner,{engagementId:engagement!.id,positionId:position,levelId:level,priorLevelId:engagement!.level_id,startDate:`${futureYear}-09-01`,currency:engagement!.currency,acceptPay:false,acceptLeave:true,commandId:`future-change-${company}`}));
  expect((await db.prepare("SELECT effective_from FROM leave_policy_assignments WHERE engagement_id=? ORDER BY effective_from DESC LIMIT 1").bind(engagement!.id).first<{effective_from:string}>())?.effective_from).toBe(`${futureYear+1}-01-01`);
  const legacyEngagement=await db.prepare("SELECT e.id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE p.user_id=? AND e.company_id=? LIMIT 1").bind(fixture.reviewer.userId,company).first<{id:string}>();
  const customEngagement=await db.prepare("SELECT e.id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE p.user_id=? AND e.company_id=? LIMIT 1").bind(owner.userId,company).first<{id:string}>();
  const legacyPolicy=`legacy-${company}`,customPolicy=`custom-${company}`;
  for(const [id,key] of [[legacyPolicy,"legacy-untracked"],[customPolicy,"custom-leave"]])await db.prepare("INSERT INTO leave_policy_versions(id,company_id,policy_key,version,name,status,effective_from,created_by,created_at) VALUES(?,?,?,1,?,'active','2020-01-01',?,1)").bind(id,company,key,key,owner.userId).run();
  for(const [id,engagementId,policyId] of [[`legacy-assignment-${company}`,legacyEngagement!.id,legacyPolicy],[`custom-assignment-${company}`,customEngagement!.id,customPolicy]])await db.prepare("INSERT INTO leave_policy_assignments(id,company_id,engagement_id,policy_version_id,effective_from,created_by,created_at) VALUES(?,?,?,?,'2020-01-01',?,1)").bind(id,company,engagementId,policyId,owner.userId).run();
  await db.batch(await recommendationStatements(db,owner,{engagementId:legacyEngagement!.id,positionId:position,levelId:level,priorLevelId:null,startDate:"2026-09-01",currency:engagement!.currency,acceptPay:false,acceptLeave:true,commandId:`legacy-${company}`}));
  await db.batch(await recommendationStatements(db,owner,{engagementId:customEngagement!.id,positionId:position,levelId:level,priorLevelId:null,startDate:"2026-09-01",currency:engagement!.currency,acceptPay:false,acceptLeave:true,commandId:`custom-${company}`}));
  const migrated=(await db.prepare("SELECT effective_from,effective_to FROM leave_policy_assignments WHERE engagement_id=? ORDER BY effective_from").bind(legacyEngagement!.id).all<{effective_from:string;effective_to:string|null}>()).results;
  expect(migrated).toMatchObject([{effective_from:"2020-01-01",effective_to:"2026-08-31"},{effective_from:"2026-09-01",effective_to:null}]);
  expect((await db.prepare("SELECT count(*) n FROM leave_policy_assignments WHERE engagement_id=?").bind(customEngagement!.id).first<{n:number}>())?.n).toBe(1);
  await savePositionPayDefault(db,owner,position,{payGroupId:group,currency:engagement!.currency,monthlyAmount:350000,expectedVersion:1},`pay-v2-${company}`);
  await saveLevelLeaveDefault(db,owner,level,{annualDays:7,sickDays:15,expectedVersion:1},`leave-v2-${company}`);
  const revised=await defaultCatalog(db,owner);
  expect(revised.pay.find(entry=>entry.position_id===position)).toMatchObject({version:2,monthly_amount_minor:350000});
  expect(revised.leave.find(entry=>entry.level_id===level)).toMatchObject({version:2,annual_days:7,sick_days:15});
  expect((await db.prepare("SELECT monthly_amount_minor FROM position_pay_defaults WHERE company_id=? AND position_id=? AND version=1").bind(company,position).first<{monthly_amount_minor:number}>())?.monthly_amount_minor).toBe(360000);
  expect((await db.prepare("SELECT policy_version_id FROM leave_policy_assignments WHERE engagement_id=? ORDER BY effective_from DESC LIMIT 1").bind(engagement!.id).first<{policy_version_id:string}>())?.policy_version_id).toBe(policy);
});
