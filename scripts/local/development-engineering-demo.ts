/** Fictional normal-login development personas. Never changes an existing user's account. */
import {randomBytes} from "node:crypto";
import {readFile,mkdir,writeFile,chmod} from "node:fs/promises";
import {join,resolve} from "node:path";
import {hashPassword} from "better-auth/crypto";
import {getRuntime,closeRuntime} from "../../src/server/runtime/env";
import {assertHospitalityDevelopmentTarget} from "../../src/lib/hospitality-seed-target";
import {seedExpenseCompany} from "../../fixtures/expenses/company";
import {mapSpendingOrganization} from "../../tests/helpers/spending-organization";
import {moduleRegistry} from "../../src/data/module-registry";
import {businessRoles} from "../../src/lib/business-roles";

const manifestPath=process.env.ERPX_DATABASE_MANIFEST;
if(!manifestPath||process.env.ERPX_PUBLIC_ORIGIN!=="https://console-dev-erpx.jesadakorn.com")throw new Error("EXACT_DEVELOPMENT_CONSOLE_REQUIRED");
const manifest=JSON.parse(await readFile(manifestPath,"utf8"));assertHospitalityDevelopmentTarget(manifest,process.env.ERPX_ENVIRONMENT);
const directory=resolve(".scratch/dev/engineering-completion"),path=join(directory,"development-demo-private.json"),env=getRuntime();
try{
 let previous:{companyId:string;users:{userId:string}[]}|null=null;
 try{previous=JSON.parse(await readFile(path,"utf8"));}catch{/* First explicit fictional provision. */}
 if(previous&&await env.H001_DB.prepare("SELECT 1 FROM organization_nodes WHERE id=? AND kind='company'").bind(previous.companyId).first()){
  for(const user of previous.users)if(!await env.CONTROL_DB.prepare("SELECT 1 FROM principals WHERE user_id=? AND status='active'").bind(user.userId).first())throw new Error("FICTIONAL_PERSONA_RECOVERY_REQUIRED");
  console.log(JSON.stringify({environment:"development",reused:true,companyId:previous.companyId,credentialFixture:path}));
 }else{
  const f=await seedExpenseCompany(env.H001_DB,env.H001_FILES);await mapSpendingOrganization(f);
  await env.H001_DB.prepare("UPDATE organization_nodes SET display_name=? WHERE id=?").bind("ERPX Engineering Demo · Fictional Company",f.company).run();
  const binding=await env.CONTROL_DB.prepare("SELECT holding_id,slug FROM holding_bindings WHERE write_binding='H001_DB' AND status='active'").first<{holding_id:string;slug:string}>();
  if(!binding||binding.holding_id!=="h001")throw new Error("ACTIVE_DEVELOPMENT_HOLDING_REQUIRED");
  const password=`Engineering-${randomBytes(20).toString("hex")}`,passwordHash=await hashPassword(password),now=Date.now(),users=[];
  for(const [label,role] of [["maker","employee"],["approver","people_manager"],["hr","hr_admin"],["finance","finance_preparer"],["finance-approver","finance_approver"],["treasury","treasury"],["legal","legal_manager"],["legal-reviewer","legal_counsel"],["administrator","holding_owner"]] as const){
   const actor=label==="administrator"?f.owner:await f.member(`engineering-${label}`,role);actor.role=role;actor.assignmentId=await f.grant(actor,role);
   const modules=role==="holding_owner"?moduleRegistry.map(item=>item.id):[...businessRoles[role]],name=`Fictional ${label}`,email=`engineering-${label}-${f.company.slice(-12)}@example.test`;
   await env.H001_DB.batch([
    env.H001_DB.prepare("UPDATE assignments SET revoked_at=? WHERE company_id=? AND user_id=? AND id<>?").bind(now,f.company,actor.userId,actor.assignmentId),
    env.H001_DB.prepare("UPDATE assignments SET modules_json=? WHERE id=?").bind(JSON.stringify(modules),actor.assignmentId),
    env.H001_DB.prepare("UPDATE hr_people SET legal_name=?,preferred_name=?,email=? WHERE company_id=? AND user_id=?").bind(name,name,email,f.company,actor.userId),
   ]);
   await env.CONTROL_DB.batch([
    env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)').bind(actor.userId,name,email,now,now),
    env.CONTROL_DB.prepare("INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?)").bind(`account-${actor.userId}`,actor.userId,actor.userId,passwordHash,now,now),
    env.CONTROL_DB.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?)").bind(`principal-${actor.userId}`,actor.userId,name,now),
   ]);
   users.push({label,name,email,userId:actor.userId,role,assignmentId:actor.assignmentId});
  }
  await mkdir(directory,{recursive:true,mode:0o700});await writeFile(path,JSON.stringify({environment:"development",synthetic:true,companyId:f.company,holdingId:binding.holding_id,holdingSlug:binding.slug,origin:process.env.ERPX_PUBLIC_ORIGIN,password,users},null,2)+"\n",{mode:0o600});await chmod(path,0o600);
  console.log(JSON.stringify({environment:"development",reused:false,companyId:f.company,personas:users.length,credentialFixture:path}));
 }
}finally{await closeRuntime();}
