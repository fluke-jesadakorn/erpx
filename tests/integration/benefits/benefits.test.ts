import {afterEach,beforeAll,beforeEach,expect,it,vi} from "vitest";
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {benefitDetail,benefitsWorkspace,benefitWork} from "@erpx/server-benefits/public/benefits";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";

type Fixture=Awaited<ReturnType<typeof expenseFixture>>;
let f:Fixture;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{
 f=await expenseFixture();
 for(const actor of [f.owner,f.tie,f.claimant,f.reviewer])actor.modules=["benefits","people_payroll"];
});
afterEach(()=>{vi.unstubAllEnvs();});

async function worker(actor:WorkContext){
 const row=await db.prepare("SELECT p.id person_id,p.legal_name,e.id engagement_id FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=? AND p.user_id=? AND p.status='active' LIMIT 1").bind(actor.companyId,actor.userId).first<{person_id:string;legal_name:string;engagement_id:string}>();
 if(!row)throw Error("ACTIVE_BENEFITS_WORKER_REQUIRED");return row;
}

/** The native Benefits owner owns every command. Retained reads are seeded from
 * published/open/verified source rows so this suite never re-creates the removed
 * TypeScript writers; the Rust command suites prove create/change/eligibility. */
async function seededBenefits(subject:WorkContext){
 const {person_id,engagement_id}=await worker(subject),plan=`plan-${key()}`,window=`window-${key()}`,dependent=`dependent-${key()}`,election=`election-${key()}`,now=Date.now();
 await db.batch([
  db.prepare("INSERT INTO benefit_plans(id,company_id,name,provider,coverage_type,currency,employee_deduction_minor,employer_cost_minor,effective_from,effective_to,status,created_by,reviewed_by,version,created_at,updated_at) VALUES(?,?,?,'Declared fixture provider','family','THB',12550,30000,'2026-01-01','2026-12-31','published',?,?,2,?,?)").bind(plan,f.company,`Fixture family plan ${plan}`,f.owner.userId,f.tie.userId,now,now+1),
  db.prepare("INSERT INTO benefit_enrollment_windows(id,company_id,name,opens_on,closes_on,effective_on,status,created_by,reviewed_by,version,created_at,updated_at) VALUES(?,?,?,'2026-01-01','2026-12-31','2026-01-01','open',?,?,2,?,?)").bind(window,f.company,`Fixture enrollment ${window}`,f.owner.userId,f.tie.userId,now+2,now+3),
  db.prepare("INSERT INTO benefit_dependents(id,company_id,person_id,name,relationship,birth_date,status,created_by,reviewed_by,version,created_at,updated_at) VALUES(?,?,?,'Fixture child','child','2018-01-01','verified',?,?,1,?,?)").bind(dependent,f.company,person_id,f.owner.userId,f.tie.userId,now+4,now+5),
  db.prepare("INSERT INTO benefit_elections(id,company_id,person_id,engagement_id,plan_id,plan_version,window_id,window_version,coverage_type,dependent_ids_json,currency,employee_deduction_minor,employer_cost_minor,effective_from,effective_to,status,created_by,version,created_at,updated_at) VALUES(?,?,?,?,?,2,?,2,'family',?,'THB',12550,30000,'2026-01-01','2026-12-31','elected',?,1,?,?)").bind(election,f.company,person_id,engagement_id,plan,window,JSON.stringify([dependent]),f.owner.userId,now+6,now+7),
  db.prepare("INSERT INTO benefit_election_events(id,election_id,version,action,actor_id,reason,status,created_at) VALUES(?,?,1,'elect',?,'Fixture employee elected reviewed coverage','elected',?)").bind(`benefit-election-event-${election}`,election,f.owner.userId,now+8),
 ]);
 return {plan,window,dependent,election,person_id,engagement_id};
}

it("lists the company Benefits workspace and enriches election names for a reviewer",async()=>{
 const seeded=await seededBenefits(f.claimant),employee=await worker(f.claimant),workspace=await benefitsWorkspace(f.owner);
 expect(workspace.plans.map(row=>row.id)).toEqual([seeded.plan]);
 expect(workspace.windows.map(row=>row.id)).toEqual([seeded.window]);
 expect(workspace.elections).toEqual([expect.objectContaining({id:seeded.election,plan_name:`Fixture family plan ${seeded.plan}`,employee_name:employee.legal_name,status:"elected",version:1})]);
 expect(workspace).toMatchObject({actorId:f.owner.userId,canWrite:true,canReview:true});
 expect(workspace.dependents).toEqual([]);
});

it("keeps My benefits personal when the actor also has benefit review authority",async()=>{
 const seeded=await seededBenefits(f.claimant),company=await benefitsWorkspace(f.owner),personal=await benefitsWorkspace(f.owner,true);
 expect(company.elections.map(row=>row.id)).toEqual([seeded.election]);
 expect(personal.elections).toEqual([]);
 expect(personal.plans.map(row=>row.id)).toEqual([seeded.plan]);
});

it("returns the retained owner detail with history and hides another employee election",async()=>{
 const seeded=await seededBenefits(f.claimant),own=await benefitDetail(f.claimant,seeded.election);
 expect(own).toMatchObject({isOwner:true,canReview:false,payroll:null});
 expect(own.election).toMatchObject({id:seeded.election,plan_name:`Fixture family plan ${seeded.plan}`,plan_status:"published",plan_current_version:2,status:"elected"});
 expect(own.history.map(row=>[row.version,row.action,row.status])).toEqual([[1,"elect","elected"]]);
 expect(await benefitDetail(f.owner,seeded.election)).toMatchObject({isOwner:false,canReview:true});
 await expect(benefitDetail(f.reviewer,seeded.election)).rejects.toMatchObject({code:"BENEFIT_ELECTION_NOT_FOUND",status:404});
});

it("derives the owner Benefits work queue from the current employee election",async()=>{
 const seeded=await seededBenefits(f.claimant),own=(await benefitWork(f.claimant)).find(row=>row.id===seeded.election)!,reviewed=(await benefitWork(f.owner)).find(row=>row.id===seeded.election)!;
 expect(own).toMatchObject({id:seeded.election,type:"benefit_enrollment",status:"elected",queue:"todo",canAct:true,relationships:["requested"]});
 expect(reviewed).toMatchObject({id:seeded.election,queue:"waiting",canAct:false,relationships:["team"]});
});

it("requires the actual Benefits addon before serving retained reads",async()=>{
 await seededBenefits(f.claimant);vi.stubEnv("ERPX_PEOPLE_PACKAGE_ENFORCEMENT","enforced");
 await expect(benefitsWorkspace(f.owner)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 await expect(benefitDetail(f.claimant,String((await db.prepare("SELECT id FROM benefit_elections WHERE company_id=? LIMIT 1").bind(f.company).first<{id:string}>())!.id))).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 expect(await benefitWork(f.claimant)).toEqual([]);
});
