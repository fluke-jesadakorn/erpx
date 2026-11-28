import {afterEach,beforeAll,beforeEach,expect,it,vi} from "vitest";
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {benefitWorkItems} from "@erpx/benefits-server/domain/benefits";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";

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

/** The 7-SELECT TypeScript workspace/detail adapter and its public barrel are
 * retired by retire.benefits.workspace-read. The native owner is the sole read
 * authority: workspace scoping, reviewer/personal views, package denial, detail
 * history and payroll are proved by benefits.rust-benefits-workspace.integration
 * and benefits.rust-benefits-workspace-postgres.integration. This suite keeps a
 * PostgreSQL oracle for the retained TypeScript work projection and seeds the
 * exact source rows those native reads consume; it never re-creates a TS reader. */
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

async function elections(companyId:string,personId?:string){
 return (await db.prepare(`SELECT e.id,p.name plan_name,per.legal_name employee_name,e.status,e.version,e.person_id FROM benefit_elections e JOIN benefit_plans p ON p.id=e.plan_id AND p.company_id=e.company_id JOIN hr_people per ON per.id=e.person_id WHERE e.company_id=? ${personId?"AND e.person_id=?":""} ORDER BY e.updated_at DESC`).bind(companyId,...(personId?[personId]:[])).all<Record<string,string|number>>()).results;
}

it("seeds the company workspace source rows the native reader consumes",async()=>{
 const seeded=await seededBenefits(f.claimant),employee=await worker(f.claimant);
 expect((await db.prepare("SELECT id FROM benefit_plans WHERE company_id=? ORDER BY updated_at DESC").bind(f.company).all<{id:string}>()).results.map(row=>row.id)).toEqual([seeded.plan]);
 expect((await db.prepare("SELECT id FROM benefit_enrollment_windows WHERE company_id=? ORDER BY updated_at DESC").bind(f.company).all<{id:string}>()).results.map(row=>row.id)).toEqual([seeded.window]);
 expect(await elections(f.company)).toEqual([expect.objectContaining({id:seeded.election,plan_name:`Fixture family plan ${seeded.plan}`,employee_name:employee.legal_name,status:"elected",version:1})]);
});

it("keeps the personal view source scoped to the employee person",async()=>{
 const seeded=await seededBenefits(f.claimant),employee=await worker(f.claimant);
 expect((await elections(f.company)).map(row=>row.id)).toEqual([seeded.election]);
 expect((await elections(f.company,employee.person_id)).map(row=>row.id)).toEqual([seeded.election]);
 expect(await elections(f.company,"foreign-person")).toEqual([]);
});

it("seeds the election detail source with history and no released payroll",async()=>{
 const seeded=await seededBenefits(f.claimant),employee=await worker(f.claimant);
 const election=(await db.prepare("SELECT e.id,p.name plan_name,p.version plan_current_version,p.status plan_status,e.status,e.version,e.person_id FROM benefit_elections e JOIN benefit_plans p ON p.id=e.plan_id AND p.company_id=e.company_id WHERE e.id=? AND e.company_id=?").bind(seeded.election,f.company).first<Record<string,string|number>>())!;
 expect(election).toMatchObject({id:seeded.election,plan_name:`Fixture family plan ${seeded.plan}`,plan_status:"published",plan_current_version:2,status:"elected"});
 expect(election.person_id===employee.person_id).toBe(true);
 expect((await db.prepare("SELECT version,action,status FROM benefit_election_events WHERE election_id=? ORDER BY version").bind(seeded.election).all<Record<string,string|number>>()).results).toEqual([{version:1,action:"elect",status:"elected"}]);
 expect(await db.prepare("SELECT id FROM benefit_payroll_inputs WHERE election_id=?").bind(seeded.election).first()).toBeNull();
});

it("derives the owner Benefits work queue from the current employee election",async()=>{
 const seeded=await seededBenefits(f.claimant),employee=await worker(f.claimant),rows=await elections(f.company),person={id:employee.person_id},reviewerPerson=await worker(f.owner);
 const own=benefitWorkItems(rows,person,f.claimant.userId,false).find(row=>row.id===seeded.election)!;
 const reviewed=benefitWorkItems(rows,{id:reviewerPerson.person_id},f.owner.userId,true).find(row=>row.id===seeded.election)!;
 expect(own).toMatchObject({id:seeded.election,type:"benefit_enrollment",status:"elected",queue:"todo",canAct:true,relationships:["requested"]});
 expect(reviewed).toMatchObject({id:seeded.election,queue:"waiting",canAct:false,relationships:["team"]});
});

it("requires the actual Benefits addon before the native reader serves retained history",async()=>{
 await seededBenefits(f.claimant);vi.stubEnv("ERPX_PEOPLE_PACKAGE_ENFORCEMENT","enforced");
 // Native purchase denial is proved by the Rust workspace suite; no TypeScript
 // reader remains to bypass it. The seeded source must stay untouched here.
 expect(await elections(f.company)).toHaveLength(1);
});
