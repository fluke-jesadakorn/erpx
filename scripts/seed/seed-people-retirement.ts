/** Fictional retirement cases only. No provider calls, messages, payments or account grants. */
import { readFileSync } from "node:fs";
import { getLocalProxy } from "../local/fixtures";
import type { PostgresDatabase } from "../../src/server/modules/runtime/adapters/runtime/postgres";
import { clearanceKinds, clearanceLabels, retirementInstant, nextRetirementDay } from "../../src/lib/people_payroll/retirement";
import { addDays } from "../../src/lib/people_payroll/people-roster";
export async function seedRetirement(db:PostgresDatabase,companyId="acme-co",today=new Intl.DateTimeFormat("en-CA",{timeZone:"Asia/Bangkok"}).format(new Date())) {
 const result: {id:string;scenario:string;created:boolean}[]=[];
 const examples=[{name:"Arun Example",scenario:"Draft retirement plan",days:60,status:"draft"},{name:"Mali Example",scenario:"Retiring in 30 days",days:30,status:"scheduled"},{name:"Niran Example",scenario:"Retiring soon; handover unassigned",days:3,status:"scheduled"},{name:"Suda Example",scenario:"Retired; financial clearance pending",days:-7,status:"completed"},{name:"Wichai Example",scenario:"Retired; clearance complete",days:-30,status:"completed"},{name:"Kanya Example",scenario:"Plan cancelled before cutoff",days:14,status:"cancelled"}];
 for(const [index,example] of examples.entries()) {
  const prefix=`demo-retirement-v1-${index+1}`,personId=`${prefix}-person`,engagementId=`${prefix}-engagement`,caseId=`${prefix}-case`;
  const created=await db.transaction(async tx=>{
   const old=await tx.prepare("SELECT created_by FROM hr_people WHERE id=?").bind(personId).first<{created_by:string}>();
   if(old){if(old.created_by!=="development-retirement-seed")throw Error("RETIREMENT_SEED_ID_COLLISION");return false;}
   const now=Date.now(),lastDay=addDays(today,example.days),cutoff=retirementInstant(nextRetirementDay(lastDay),"Asia/Bangkok"),creator="development-retirement-seed";
   await tx.prepare("INSERT INTO hr_people(id,company_id,legal_name,email,country_code,status,created_by,created_at,updated_at) VALUES(?,?,?,?,'TH','active',?,?,?)").bind(personId,companyId,`Demo · ${example.name}`,`${prefix}@example.invalid`,creator,now,now).run();
   await tx.prepare("INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,end_date,job_title,currency,created_by,created_at,updated_at) VALUES(?,?,?,'employee','salaried',?,'2020-01-01',?,'Fictional retirement example','THB',?,?,?)").bind(engagementId,personId,companyId,example.status==="completed"?"ended":"active",example.status==="completed"?lastDay:null,creator,now,now).run();
   const definition={kind:"offboarding",departureReason:"retirement",personId,engagementId,positionId:null,managerPersonId:null,effectiveAt:cutoff,lastWorkingDate:lastDay,timezone:"Asia/Bangkok",clearanceOwners:{equipment:null,payroll:null,finance:null},reason:example.scenario,expectedVersion:0};
   await tx.prepare("INSERT INTO people_lifecycle_cases(id,company_id,person_id,engagement_id,kind,status,version,effective_at,definition,created_by,updated_at,departure_reason) VALUES(?,?,?,?,'offboarding',?,1,?,?::jsonb,?,?,'retirement')").bind(caseId,companyId,personId,engagementId,example.status,cutoff,JSON.stringify(definition),creator,now).run();
   if(example.status!=="draft") {
    await tx.prepare("INSERT INTO people_access_cutoffs(company_id,person_id,case_id,effective_at,cancelled_at) VALUES(?,?,?,?,?)").bind(companyId,personId,caseId,cutoff,example.status==="cancelled"?now:null).run();
    for(const kind of clearanceKinds){
     const closed=index===4 || index===3&&!['payroll','finance'].includes(kind),status=example.status==="cancelled"?"cancelled":closed?"not_required":"pending";
     await tx.prepare("INSERT INTO people_preparation_tasks(id,company_id,case_id,kind,title,status,assignee_id,updated_at,privileged,due_date,responsibility,clearance_type,completion_reason,completed_by,completed_at) VALUES(?,?,?,'lifecycle',?,?,NULL,?,?,?,?,?,?,?,?)").bind(`${prefix}-${kind}`,companyId,caseId,clearanceLabels[kind],status,now,kind==="directory",lastDay,kind,kind,closed?"Fictional development scenario: no real assets, balances or accounts.":"",closed?creator:null,closed?now:null).run();
    }
   }
   return true;
  });
  result.push({id:caseId,scenario:example.scenario,created});
 }
 return result;
}
if(import.meta.main){
 const manifest=JSON.parse(readFileSync(process.env.ERPX_DATABASE_MANIFEST!,"utf8")),target=new URL(manifest.databases.H001_DB);
 if(process.env.NODE_ENV==="production"||process.env.ERPX_ENVIRONMENT!=="local"||!["127.0.0.1","localhost"].includes(target.hostname)||!target.pathname.startsWith("/erpx_dev_"))throw Error("RETIREMENT_SEED_REQUIRES_LOCAL_DEVELOPMENT_DATABASE");
 const platform=await getLocalProxy();
 try{console.log(JSON.stringify(await seedRetirement(platform.env.H001_DB as PostgresDatabase),null,2));}finally{await platform.dispose();}
}
