import assert from "node:assert/strict";
import { getLocalProxy } from "./local/fixtures";
import { drainPeopleLifecycle } from "../src/server/people-lifecycle-jobs";
import type { PostgresDatabase } from "../src/server/runtime/postgres";
const origin=process.env.ERPX_TEST_BASE_URL??"http://localhost:3100";
if(!["localhost","127.0.0.1"].includes(new URL(origin).hostname))throw new Error("Local verification only.");
const jars=new Map<string,Map<string,string>>();
const contexts=new Map<string,{revision:number;company:string;assignment:string}>();
function remember(who:string,r:Response){const jar=jars.get(who)??new Map<string,string>();for(const v of r.headers.getSetCookie()){const pair=v.split(";")[0],i=pair.indexOf("=");jar.set(pair.slice(0,i),pair.slice(i+1));}jars.set(who,jar);}
async function api(who:string,path:string,input?:unknown,status=200,key=crypto.randomUUID(),method=input?"POST":"GET") {
 if(who==="new"&&path.startsWith("/api/v1/holdings/northstar/"))path=path.replace("/northstar/","/northstar/branches/rotterdam/");
 if(who==="olivia"&&path.startsWith("/api/v1/holdings/northstar/")&&!path.includes("/branches/"))path=path.replace("/northstar/","/northstar/branches/toronto/");
 const context=contexts.get(who);const r=await fetch(`${origin}${path}`,{method,headers:{Origin:origin,"Content-Type":"application/json","Idempotency-Key":key,Cookie:[...(jars.get(who)??new Map())].map(([k,v])=>`${k}=${v}`).join("; "),...(context?{"x-erpx-context-revision":String(context.revision),"x-erpx-context-company":context.company,"x-erpx-context-assignment":context.assignment}: {})},...(input?{body:JSON.stringify(input)}:{})});remember(who,r);const data=await r.json() as {data:Record<string,unknown>;error?:{message:string}};assert.equal(r.status,status,`${path}: ${JSON.stringify(data.error)}`);return data.data;
}
const base="/api/v1/holdings/northstar";
async function selectWorkRole(who:string,assignment?:string){const active=await api(who,`${base}/assignments/active`);const selected=assignment??(active.assignmentId as string|null)??((active.options as Record<string,unknown>[])[0]?.id as string|undefined);assert.ok(selected,`No active work role for ${who}`);const chosen=await api(who,`${base}/assignments/active`,{assignmentId:selected,revision:Number(active.revision??0)});contexts.set(who,{revision:Number(chosen.revision),company:"northstar-co",assignment:String(chosen.assignmentId)});}
for(const who of ["sam","mia","olivia"]){await api(who,"/api/auth/sign-in/email",{email:`${who}@erpx.dev`,password:`${who[0].toUpperCase()+who.slice(1)}-ERPX-2026!`});}
const platform=await getLocalProxy({persist:true,remoteBindings:false});
try {
 const db=platform.env.H002_DB;
 const control=platform.env.CONTROL_DB;
 const samSession=await control.prepare('SELECT id FROM session WHERE "userId"=? ORDER BY "createdAt" DESC LIMIT 1').bind("usr_sam").first<{id:string}>();assert.ok(samSession);
 const miaSession=await control.prepare('SELECT id FROM session WHERE "userId"=? ORDER BY "createdAt" DESC LIMIT 1').bind("usr_mia").first<{id:string}>();assert.ok(miaSession);
 // Local acceptance fixture: the sessions are marked freshly verified so the test exercises the independent approval path without a real authenticator device.
 await control.prepare("INSERT INTO iam_mfa(user_id,secret,enabled) VALUES(?,?,true) ON CONFLICT(user_id) DO UPDATE SET enabled=true").bind("usr_sam","acceptance-fixture").run();
 await control.prepare("INSERT INTO iam_mfa(user_id,secret,enabled) VALUES(?,?,true) ON CONFLICT(user_id) DO UPDATE SET enabled=true").bind("usr_mia","acceptance-fixture").run();
 await control.prepare("INSERT INTO iam_mfa_sessions(session_id,user_id,verified_at) VALUES(?,?,?) ON CONFLICT(session_id) DO UPDATE SET verified_at=excluded.verified_at").bind(samSession.id,"usr_sam",Date.now()).run();
 await control.prepare("INSERT INTO iam_mfa_sessions(session_id,user_id,verified_at) VALUES(?,?,?) ON CONFLICT(session_id) DO UPDATE SET verified_at=excluded.verified_at").bind(miaSession.id,"usr_mia",Date.now()).run();
 const position=await db.prepare("SELECT id,level_id FROM organization_positions WHERE company_id='northstar-co' AND branch_id='rotterdam' AND department_id='dept:rotterdam:operations' AND title='Senior Inventory Planner' LIMIT 1").first<{id:string;level_id:string}>();assert.ok(position);
 const manager=await db.prepare("SELECT id FROM hr_people WHERE company_id='northstar-co' AND user_id='usr_sam' AND merged_into_id IS NULL").first<{id:string}>();assert.ok(manager);
 await db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES('acceptance-mia-hr','usr_mia','hr_admin','[\"people_payroll\"]','northstar-co',0,?,'holding') ON CONFLICT(id) DO UPDATE SET valid_until=excluded.valid_until,revoked_at=NULL").bind(Date.now()+365*86400000).run();
 await db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES('acceptance-mia-owner','usr_mia','holding_owner','[]','northstar-co',0,?,'holding') ON CONFLICT(id) DO UPDATE SET valid_until=excluded.valid_until,revoked_at=NULL").bind(Date.now()+365*86400000).run();
 await db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES('acceptance-sam-manager','usr_sam','people_manager','[]','northstar-co',0,?,'holding') ON CONFLICT(id) DO UPDATE SET valid_until=excluded.valid_until,revoked_at=NULL").bind(Date.now()+365*86400000).run();
 const owner=await db.prepare("SELECT id FROM assignments WHERE user_id='usr_sam' AND role='holding_owner' AND revoked_at IS NULL LIMIT 1").first<{id:string}>();assert.ok(owner);
 await selectWorkRole("sam",owner.id);await selectWorkRole("mia","acceptance-mia-hr");await selectWorkRole("olivia");
 for(const failedId of ["person-fc36b7bb-48b2-4f8f-8a79-26b97c5a7e49"]){const failed=await db.prepare("SELECT version FROM hr_people WHERE id=? AND status='active' AND legal_name='Robin Anders' AND email LIKE 'robin.%@example.test'").bind(failedId).first<{version:number}>();if(failed)await api("sam",`${base}/people/directory/${failedId}/status`,{status:"archived",expectedVersion:failed.version});}
 const suffix=crypto.randomUUID().slice(0,8),email=`robin.${suffix}@example.test`,key=crypto.randomUUID();
 const input={legalName:"Robin Anders",preferredName:"Robin",email,phone:"",countryCode:"NL",branchId:"rotterdam",departmentId:"dept:rotterdam:operations",jobTitle:"Inventory Planner",workerType:"employee",employmentType:"salaried",startDate:"2026-09-07",currency:"EUR",managerPersonId:manager.id,levelId:String(position.level_id),positionId:position.id};
 const person=await api("mia",`${base}/people/directory`,input,201,key);
 assert.deepEqual(await api("mia",`${base}/people/directory`,input,201,key),person);
 await api("olivia",`${base}/people/directory`,input,403);
 await api("mia",`${base}/people/directory/${person.id}`,{...input,jobTitle:"Senior Inventory Planner",expectedVersion:1});
 await api("mia",`${base}/people/directory/${person.id}`,{...input,expectedVersion:1},409);
 const grant={role:"employee",scope:"branch",expiresAt:"2027-08-31"};
 await api("sam",`${base}/people/directory/${person.id}/status`,{status:"active",expectedVersion:2});
 await api("olivia",`${base}/people/directory/${person.id}/invite`,grant,403);
 const invitation=await api("sam",`${base}/people/directory/${person.id}/invite`,grant);
 await selectWorkRole("mia","acceptance-mia-owner");
 await api("mia",`${base}/iam/reviews/${invitation.reviewId}`,{expectedVersion:1,decision:"approve",reason:"Independent administrator approved the employee invitation."});
 await selectWorkRole("mia","acceptance-mia-hr");
 const password=`Acceptance-${crypto.randomUUID()}!`;
 await api("new","/api/v1/account-invitations",{token:invitation.token,password});
 await api("new","/api/v1/account-invitations",{token:invitation.token,password},410);
 await api("new","/api/auth/sign-in/email",{email,password});
 await selectWorkRole("new");
 const profile=await api("new",`${base}/people/people/${person.id}`);assert.ok(profile);
 const engagement=await db.prepare("SELECT id FROM hr_engagements WHERE person_id=? AND company_id='northstar-co' AND status='active'").bind(person.id).first<{id:string}>();assert.ok(engagement);
 // The directory flow creates the active engagement; HR then assigns the existing
 // company policy before the employee can submit leave.
 await db.prepare("INSERT INTO leave_policy_assignments(id,company_id,engagement_id,policy_version_id,effective_from,created_by,created_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING").bind(`acceptance-leave-${person.id}`,"northstar-co",engagement.id,"leave-policy-legacy-northstar-co","2026-09-07","usr_mia",Date.now()).run();
 await api("new","/api/v1/holdings/acme/people/summary",undefined,409);
 const privatePerson=await db.prepare("SELECT id FROM hr_people WHERE legal_name='Sam Rivera' AND merged_into_id IS NULL").first<{id:string}>();assert.ok(privatePerson);
 const other=await api("new",`${base}/people/people/${privatePerson.id}`);assert.equal((other.person as Record<string,unknown>).base_amount_minor,undefined);assert.deepEqual(other.time,[]);
 const run=await db.prepare("SELECT id FROM hr_payroll_runs LIMIT 1").first<{id:string}>();assert.ok(run);await api("new",`${base}/people/payroll/runs/${run.id}`,undefined,403);
 const leave=await api("new",`${base}/leave-requests`,{startDate:"2026-10-05",days:2,handoverNote:"Planning queue transferred to the shift lead."},201);
 await api("new",`${base}/leave-requests/${leave.id}`,{startDate:"2026-10-05",days:2,handoverNote:"Planning queue transferred to the shift lead.",expectedVersion:1},200,crypto.randomUUID(),"PATCH");
 await api("new",`${base}/leave-requests/${leave.id}/approve`,{expectedVersion:2,note:"self review"},403);
 await api("olivia",`${base}/leave-requests/${leave.id}`,undefined,404);
 await selectWorkRole("sam","acceptance-sam-manager");
 await api("sam",`${base}/leave-requests/${leave.id}/approve`,{expectedVersion:2,note:"Coverage confirmed."});
 await api("mia",`${base}/leave-requests/${leave.id}/approve`,{expectedVersion:3,note:"HR policy confirmed."});
 const user=await db.prepare("SELECT user_id,version FROM hr_people WHERE id=?").bind(person.id as string).first<{user_id:string;version:number}>();assert.ok(user);
 const assignment=await db.prepare("SELECT id FROM assignments WHERE user_id=? AND revoked_at IS NULL").bind(user.user_id).first<{id:string}>();assert.ok(assignment);
 await selectWorkRole("sam",owner.id);
 await api("sam",`${base}/people/directory/${person.id}/revoke`,{assignmentId:assignment.id});
 await api("new",`${base}/people/summary`,undefined,409);
 const lifecycleInput={expectedVersion:0,kind:"offboarding",personId:person.id,engagementId:engagement.id,positionId:position.id,managerPersonId:manager.id,effectiveAt:Date.now()-1000,departureReason:"other",timezone:"Europe/Amsterdam",clearanceOwners:{equipment:null,payroll:null,finance:null},lastWorkingDate:"2026-09-22",reason:"Acceptance scenario employee departure"};
 const lifecyclePreview=await api("sam",`${base}/people/lifecycle/cases/preview`,lifecycleInput);
 const lifecycleCase=await api("sam",`${base}/people/lifecycle/cases/submit`,{...lifecycleInput,previewToken:lifecyclePreview.token});assert.equal(lifecycleCase.status,"scheduled");
 await drainPeopleLifecycle(db as unknown as PostgresDatabase,Date.now());
 const ended=await db.prepare("SELECT status,version FROM hr_engagements WHERE id=?").bind(engagement.id).first<{status:string;version:number}>();assert.equal(ended?.status,"ended");
 await api("sam",`${base}/people/directory/${person.id}/status`,{status:"archived",expectedVersion:user.version});
 console.log(JSON.stringify({result:"PASS",personId:person.id,checks:["employee create and replay","optimistic edit conflict","HR cannot grant access","owner invitation activation","single-use activation","password login","cross-holding denial","compensation privacy","payroll privacy","employee leave submit","self-approval denial","leave privacy","independent approval","revoked-session access denial","archive with history"]},null,2));
}finally{await platform.env.H002_DB.prepare("UPDATE assignments SET revoked_at=? WHERE id IN ('acceptance-mia-hr','acceptance-mia-owner','acceptance-sam-manager')").bind(Date.now()).run();await platform.dispose();}
