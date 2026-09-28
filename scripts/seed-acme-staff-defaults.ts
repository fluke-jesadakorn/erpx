import { readFileSync } from "node:fs";
import { getLocalProxy } from "./local/fixtures";
import { defaultCatalog } from "../src/server/staff-defaults";
import { runLeaveAccruals } from "../src/server/leave-self-service";
import type { SqlStatement } from "../src/server/runtime/types";

const mode=process.argv[2]??"--preview";
if(!["--preview","--defaults","--apply","--verify"].includes(mode))throw new Error("Use --preview, --defaults, --apply, or --verify.");
const company="acme-co",today=new Date().toISOString().slice(0,10),now=Date.now();
const annual=[18,15,12,10,8,6],pay=[12000,9000,6500,4800,3600,2800];
const proxy=await getLocalProxy();
try{
  const path=process.env.ERPX_DATABASE_MANIFEST;
  if(!path)throw new Error("Development database manifest is required.");
  const manifest=JSON.parse(readFileSync(path,"utf8")) as {databases:Record<string,string>};
  for(const key of ["CONTROL_DB","H001_DB","H002_DB"]){
    const url=new URL(manifest.databases[key]);
    if(!["127.0.0.1","localhost"].includes(url.hostname)||url.port!=="5432"||!url.pathname.startsWith("/erpx_dev_"))throw new Error("ACME_DEV_DATABASES_ONLY");
  }
  if(proxy.env.ERPX_ENVIRONMENT!=="local")throw new Error("LOCAL_DEVELOPMENT_ONLY");
  const db=proxy.env.H001_DB;
  const levels=(await db.prepare("SELECT id,level_number FROM staff_levels WHERE company_id=? AND active=1 AND level_number BETWEEN 1 AND 6 ORDER BY level_number").bind(company).all<{id:string;level_number:number}>()).results;
  const positions=(await db.prepare("SELECT id,title,level_id FROM organization_positions WHERE company_id=? AND active=1 ORDER BY id").bind(company).all<{id:string;title:string;level_id:string}>()).results;
  const groups=(await db.prepare("SELECT id FROM hr_pay_groups WHERE company_id=? AND currency='USD' AND frequency='monthly' AND status='active' ORDER BY id").bind(company).all<{id:string}>()).results;
  const employees=(await db.prepare("SELECT e.id,e.position_id,e.level_id,e.start_date,e.currency,e.pay_group_id FROM hr_engagements e WHERE e.company_id=? AND e.status IN ('active','onboarding','suspended') AND e.worker_type='employee' ORDER BY e.id").bind(company).all<{id:string;position_id:string|null;level_id:string|null;start_date:string;currency:string;pay_group_id:string|null}>()).results;
  const missing={positions:employees.filter(e=>!e.position_id).map(e=>e.id),levels:employees.filter(e=>!e.level_id).map(e=>e.id),payGroup:employees.filter(e=>!e.pay_group_id).map(e=>e.id)};
  const report={mode,company,activePositions:positions.length,employees:employees.length,missing};
  if(levels.length!==6||!groups.length)throw new Error(JSON.stringify({error:"Missing six levels or active USD monthly pay group",report}));
  const group=groups[0].id;
  const actor={companyId:company,role:"holding_owner"} as Parameters<typeof defaultCatalog>[1];
  let defaults=await defaultCatalog(db,actor);
  const plannedPay=positions.filter(p=>!defaults.pay.some(d=>d.position_id===p.id));
  const plannedLeave=levels.filter(l=>!defaults.leave.some(d=>d.level_id===l.id));
  console.log(JSON.stringify({...report,missingPositionPay:plannedPay.length,missingLevelLeave:plannedLeave.map(l=>l.level_number)}));
  if(mode==="--preview")process.exit(0);
  if(mode==="--verify"){
    if(plannedPay.length||plannedLeave.length)throw new Error("Defaults are incomplete.");
    process.exit(0);
  }
  const types=(await db.prepare("SELECT id,code,tracked FROM leave_types WHERE company_id=? AND code IN ('annual','sick')").bind(company).all<{id:string;code:string;tracked:boolean}>()).results;
  if(types.some(t=>!t.tracked))throw new Error("Existing annual or sick leave type is untracked; review manually.");
  const statements:SqlStatement[]=[];
  for(const [code,name] of [["annual","Annual leave"],["sick","Sick leave"]])if(!types.some(t=>t.code===code)){
    statements.push(db.prepare("INSERT INTO leave_types(id,company_id,code,name,description,tracked,effective_from,created_by,created_at,updated_at) VALUES(?,?,?,?,?,true,?,'local-demo-seed',?,?)")
      .bind(`acme-${code}-type`,company,code,name,"Fictional Acme development fixture",today,now,now));
  }
  for(const p of plannedPay){
    const level=levels.find(l=>l.id===p.level_id);
    if(!level)continue;
    statements.push(db.prepare("INSERT INTO position_pay_defaults(id,company_id,position_id,version,pay_group_id,currency,monthly_amount_minor,created_by,created_at) VALUES(?,?,?,1,?,'USD',?,'local-demo-seed',?) ON CONFLICT(company_id,position_id,version) DO NOTHING")
      .bind(`acme-default-pay-${p.id}`,company,p.id,group,pay[level.level_number-1]*100,now));
  }
  await db.batch(statements);
  const leaveTypes=(await db.prepare("SELECT id,code FROM leave_types WHERE company_id=? AND code IN ('annual','sick') AND tracked=true").bind(company).all<{id:string;code:string}>()).results;
  for(const level of plannedLeave){
    const version=1,policyId=`acme-level-${level.level_number}-policy-v1`,mappingId=`acme-level-${level.level_number}-default-v1`;
    const batch:SqlStatement[]=[db.prepare("INSERT INTO leave_policy_versions(id,company_id,policy_key,version,name,status,effective_from,timezone,workweek_json,created_by,created_at) VALUES(?,?,?,1,?,'active',?,'UTC','[1,2,3,4,5]'::jsonb,'local-demo-seed',?) ON CONFLICT(company_id,policy_key,version) DO NOTHING")
      .bind(policyId,company,`level-${level.level_number}-default`,`Level ${level.level_number} default leave`,today,now)];
    for(const type of leaveTypes)batch.push(db.prepare("INSERT INTO leave_policy_rules(policy_version_id,leave_type_id,grant_kind,grant_units,accrual_units,employee_eligible,contractor_eligible) VALUES(?,?,'annual',?,0,true,false) ON CONFLICT DO NOTHING")
      .bind(policyId,type.id,type.code==="annual"?annual[level.level_number-1]:15));
    batch.push(db.prepare("INSERT INTO level_leave_defaults(id,company_id,level_id,version,policy_version_id,created_by,created_at) VALUES(?,?,?,? ,?,'local-demo-seed',?) ON CONFLICT(company_id,level_id,version) DO NOTHING")
      .bind(mappingId,company,level.id,version,policyId,now));
    await db.batch(batch);
  }
  if(mode==="--defaults")process.exit(0);
  defaults=await defaultCatalog(db,actor);
  let payDrafts=0,leaveAssignments=0,legacyMoved=0;
  for(const employee of employees){
    const position=positions.find(p=>p.id===employee.position_id),amount=defaults.pay.find(d=>d.position_id===employee.position_id);
    if(position&&amount&&employee.currency==="USD"&&(!employee.pay_group_id||employee.pay_group_id===amount.pay_group_id)&&!await db.prepare("SELECT 1 FROM hr_compensation_revisions WHERE engagement_id=? LIMIT 1").bind(employee.id).first()){
      if(!employee.pay_group_id)await db.prepare("UPDATE hr_engagements SET pay_group_id=? WHERE id=? AND pay_group_id IS NULL").bind(amount.pay_group_id,employee.id).run();
      await db.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,hourly_rate_minor,expected_hours,currency,components_json,status,created_by,created_at,updated_at) VALUES(?,?,?,'monthly',?,0,0,'USD','[]','draft','local-demo-seed',?,?) ON CONFLICT(id) DO NOTHING")
        .bind(`acme-default-comp-${employee.id}`,employee.id,employee.start_date,amount.monthly_amount_minor,now,now).run();
      payDrafts++;
    }
    const policy=defaults.leave.find(d=>d.level_id===employee.level_id);
    if(!policy)continue;
    const existing=await db.prepare("SELECT a.id,a.effective_from,a.effective_to,p.policy_key FROM leave_policy_assignments a JOIN leave_policy_versions p ON p.id=a.policy_version_id WHERE a.company_id=? AND a.engagement_id=? AND (a.effective_to IS NULL OR a.effective_to>=?) ORDER BY a.effective_from DESC LIMIT 1")
      .bind(company,employee.id,today).first<{id:string;effective_from:string;effective_to:string|null;policy_key:string}>();
    if(existing&&existing.policy_key!=="legacy-untracked")continue;
    const effective=employee.start_date>today?employee.start_date:today;
    if(existing&&existing.effective_from>=effective)continue;
    if(existing){
      const previous=new Date(Date.parse(`${effective}T00:00:00Z`)-86400000).toISOString().slice(0,10);
      if(existing.effective_from<=previous)await db.prepare("UPDATE leave_policy_assignments SET effective_to=?,version=version+1 WHERE id=?").bind(previous,existing.id).run();
      legacyMoved++;
    }
    await db.prepare("INSERT INTO leave_policy_assignments(id,company_id,engagement_id,policy_version_id,effective_from,created_by,created_at) VALUES(?,?,?,?,?,'local-demo-seed',?) ON CONFLICT(id) DO NOTHING")
      .bind(`acme-default-leave-${employee.id}`,company,employee.id,policy.policy_version_id,effective,now).run();
    leaveAssignments++;
  }
  const grants=await runLeaveAccruals(db,today,company);
  console.log(JSON.stringify({company,payDrafts,leaveAssignments,legacyMoved,grants,missing}));
}finally{await proxy.dispose();}
