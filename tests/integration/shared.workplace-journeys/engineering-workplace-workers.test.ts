import {beforeAll,beforeEach,afterEach,afterAll,it,expect,vi} from 'vitest';
import {mkdir,writeFile,rm} from 'node:fs/promises';
import {createHash,randomBytes} from 'node:crypto';
import {join,resolve} from 'node:path';
import {Pool} from 'pg';
import {PostgresDatabase} from "@erpx/server-runtime/adapters/runtime/postgres";
import {LocalFileStore} from "@erpx/server-runtime/adapters/runtime/filesystem";
import {applyMigrations,env} from "../../support/runtime-env";
import {seedExpenseCompany} from "../../../fixtures/expenses/company";
import {enrollEngineeringVerification} from "@erpx/server-platform_administration/public/engineering-verification";
import {saveMaster,saveOrder,orderSchema} from '@erpx/server-commerce/public/commerce';
import {createSupportTicket,getSupportTicket} from '@erpx/server-support/public/support';
import {createSupportPolicy,changeSupportPolicy,attachSupportClock} from "@erpx/server-support/public/support-sla";
import {drainSupportSla} from "@erpx/server-support/public/support-sla-alerts";
import {savePostDraft,publishPost,postView} from '@erpx/server-collaboration/public/social-home';
import {drainSocialAgent} from "@erpx/server-collaboration/public/social-agent";
import {createReport,reportView} from "@erpx/server-runtime/public/executive/reports";
import {queueGeneration,drainExecutiveGeneration} from "@erpx/server-runtime/public/executive/generation";
import {generateSchema,emptyDefinition} from "@erpx/feature-shared/lib/executive/canvas";
import {selectAiProviders,aiSelectionSchema} from "@erpx/server-agent/public/ai-providers";

const run=randomBytes(6).toString('hex'),source='a'.repeat(64),version=`release-${source}`,directory=resolve('.runtime/browser',run),databaseName=`erpx_browser_${run}_h001_db`,key=()=>crypto.randomUUID();
const aiEnv={AI_BASE_URL:'https://opencode.ai/zen/go/v1',AI_MODEL:'muse-spark-1.3-contributor',AI_API_KEY:'declared-no-network-worker-fixture'};
vi.mock("@erpx/catalogue/release-artifact.json",()=>({default:{softwareVersion:`release-${'a'.repeat(64)}`}}));
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:db,files}),runtimeEnv:async()=>({...env,H001_DB:db,H001_FILES:files})}));
let db:PostgresDatabase,files:LocalFileStore,admin:Pool,manifestSha:string,f:Awaited<ReturnType<typeof seedExpenseCompany>>,disabled:typeof f,unenrolled:typeof f;
beforeAll(async()=>{
 const url=new URL(process.env.ERPX_TEST_ADMIN_URL!);if(url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||url.pathname!=='/postgres')throw new Error('EXACT_LOCAL_TEST_ADMIN_REQUIRED');
 admin=new Pool({connectionString:url.href,max:1});await admin.query(`CREATE DATABASE "${databaseName}" TEMPLATE template0`);url.pathname=databaseName;db=new PostgresDatabase({connectionString:url.href,max:4});files=new LocalFileStore(join(directory,'files/h001'),db);
 await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);await mkdir(join(directory,'release'),{recursive:true});const manifest=JSON.stringify({'tools/jobs.mjs':'b'.repeat(64)});manifestSha=createHash('sha256').update(manifest).digest('hex');
 await writeFile(join(directory,'release/release-files.json'),manifest);await writeFile(join(directory,'release/release.json'),JSON.stringify({sourceFingerprint:source,softwareVersion:version,fileManifestSha256:manifestSha}));await writeFile(join(directory,'engineering-attestation.json'),JSON.stringify({scope:'isolated-engineering',run,sourceFingerprint:source,softwareVersion:version,fileManifestSha256:manifestSha}));
});
beforeEach(async()=>{vi.stubEnv('NODE_ENV','development');f=await seedExpenseCompany(db,files);disabled=await seedExpenseCompany(db,files);unenrolled=await seedExpenseCompany(db,files);});
afterEach(()=>{vi.restoreAllMocks();vi.unstubAllEnvs();});
afterAll(async()=>{await db?.close();if(admin){const row=(await admin.query('SELECT pg_get_userbyid(datdba) owner FROM pg_database WHERE datname=$1',[databaseName])).rows[0];if(row?.owner!=='erpx_test_admin')throw new Error('ISOLATED_WORKER_DATABASE_OWNER_CHANGED');await admin.query(`DROP DATABASE "${databaseName}"`);await admin.end();}await rm(directory,{recursive:true,force:true});});
async function production(){
 vi.stubEnv('NODE_ENV','production');vi.stubEnv('ERPX_ENVIRONMENT','local');vi.stubEnv('PORT','3104');vi.stubEnv('ERPX_BROWSER_SIMULATION_PORT','3104');vi.stubEnv('ERPX_ENGINEERING_VERIFICATION_RUN',run);vi.stubEnv('ERPX_DATA_DIR',directory);
 await enrollEngineeringVerification(db,{run,sourceFingerprint:source,softwareVersion:version,fileManifestSha256:manifestSha,companyIds:[f.company],disabledCompanyIds:[disabled.company]});
}
async function supportTicket(company:typeof f){
 const {owner,head,manager}=company;for(const [actor,role] of [[head,'support_agent'],[manager,'support_manager']] as const){actor.role=role;actor.assignmentId=await company.grant(actor,role);actor.branchId=null;actor.departmentId=null;}
 const customer=await saveMaster(owner,'parties',null,{kind:'customer',code:key(),name:'Fictional SLA customer'},key()),warehouse=await saveMaster(owner,'warehouses',null,{branchId:company.branch,code:key(),name:'Fictional SLA warehouse'},key()),product=await saveMaster(owner,'products',null,{sku:key(),name:'Fictional SLA equipment',price:'10'},key());
 const order=await saveOrder(owner,null,orderSchema.parse({kind:'quote',partyId:customer.id,warehouseId:warehouse.id,orderDate:'2026-09-20',dueDate:'2026-09-23',lines:[{productId:product.id,quantity:'1',price:'10',taxBps:0}]}),key());
 const ticket=await createSupportTicket(owner,{orderId:order.id,sourceVersion:1,title:'Fictional overdue support ticket',issue:'Retain the linked native customer request',ownerId:head.userId,dueDate:'2026-10-10'},key()),policy=await createSupportPolicy(owner,{name:'Fictional one-minute engineering SLA',policy:{calendar:{timezone:'Asia/Bangkok',weekdays:[0,1,2,3,4,5,6],opensAtMinute:0,closesAtMinute:1440,holidays:[]},targetMinutes:1,pauseWhileWaiting:true},reason:'Declared fixture calendar; no real SLA contract'},key());
 await changeSupportPolicy(manager,String(policy.id),{action:'approve',expectedVersion:1,reason:'Independent fixture calendar review'},key());const clock=vi.spyOn(Date,'now').mockReturnValue(Date.now()-3600000);try{await attachSupportClock(owner,String(ticket.id),{expectedVersion:1,policyId:policy.id,policyVersion:2,reason:'Declared historical fixture clock'},key());}finally{clock.mockRestore();}
 return String(ticket.id);
}
async function social(company:typeof f){
 const draft=await savePostDraft(db,company.owner,null,{body:'@ERPX assistant Help with this fictional manual question',mentions:[{kind:'agent',targetId:'erpx-assistant',label:'ERPX assistant',start:0,end:15}],branchId:company.branch,expectedVersion:1},key()),post=await publishPost(db,company.owner,String(draft.id),Number(draft.version),key());
 return {id:String(post.id),reply:(await postView(db,company.owner,String(post.id))).agentReplies[0].id};
}
async function executive(company:typeof f){await selectAiProviders(db,company.owner,aiSelectionSchema.parse({expectedVersion:0,mode:'deployment',imageMode:'disabled'}),key());const report=await createReport(db,company.owner,key(),'Fictional manual report'),job=key();await queueGeneration(db,company.owner,aiEnv,report.id,generateSchema.parse({question:'Propose a fictional layout for explicit human review'}),job);return {report:report.id,job};}
const reply=()=>({result:{body:'Declared fixture reply; a person decides the next step.'},inputTokens:4,outputTokens:3});
const proposal=()=>({result:{message:'Declared fixture proposal; review before applying.',definition:emptyDefinition('Fictional proposal'),followUps:[]},inputTokens:4,outputTokens:3});

it('delivers one enabled Support breach to the current manager across retries while disabled and unenrolled Companies stay untouched',async()=>{
 const ticket=await supportTicket(f),blocked=await supportTicket(disabled),absent=await supportTicket(unenrolled),before=await getSupportTicket(f.head,ticket);await production();
 await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(Date.now(),f.manager.assignmentId).run();await Promise.all([drainSupportSla(db),drainSupportSla(db)]);
 expect(await db.prepare('SELECT count(*) n FROM support_sla_alerts WHERE ticket_id=?').bind(ticket).first('n')).toBe(1);expect(await db.prepare("SELECT count(*) n FROM staff_notifications WHERE record_type='support_sla_alert' AND company_id=? AND user_id=?").bind(f.company,f.manager.userId).first('n')).toBe(0);
 f.manager.assignmentId=await f.grant(f.manager,'support_manager');await drainSupportSla(db);await drainSupportSla(db);
 const notices=(await db.prepare("SELECT user_id,record_id FROM staff_notifications WHERE record_type='support_sla_alert' AND company_id=?").bind(f.company).all<{user_id:string;record_id:string}>()).results;expect(notices).toHaveLength(2);expect(notices).toEqual(expect.arrayContaining([f.manager.userId,f.owner.userId].map(user_id=>({user_id,record_id:`support-breach-${ticket}`}))));expect(await getSupportTicket(f.head,ticket)).toEqual(before);
 expect(await db.prepare('SELECT count(*) n FROM support_sla_alerts WHERE ticket_id IN (?,?)').bind(blocked,absent).first('n')).toBe(0);expect(await db.prepare('SELECT count(*) n FROM company_release_acceptances').first('n')).toBe(0);
});
it('publishes one explicitly requested Social reply with a single usage receipt while disabled claims and lease recovery remain frozen',async()=>{
 const post=await social(f),blocked=await social(disabled),absent=await social(unenrolled);await db.prepare("UPDATE social_agent_replies SET state='running',lease_until=1 WHERE id=?").bind(blocked.reply).run();const before=await db.prepare('SELECT * FROM social_agent_replies WHERE id=?').bind(blocked.reply).first();await production();const provider=vi.fn(async()=>reply());
 await Promise.all([drainSocialAgent(db,aiEnv,undefined,provider),drainSocialAgent(db,aiEnv,undefined,provider)]);await drainSocialAgent(db,aiEnv,undefined,provider);expect(provider).toHaveBeenCalledTimes(1);
 const view=await postView(db,f.owner,post.id);expect(view.agentReplies[0].state).toBe('completed');expect(view.comments).toHaveLength(1);expect(view.comments[0]).toMatchObject({author_kind:'agent',body:reply().result.body});expect(await db.prepare('SELECT state,output_tokens FROM social_ai_usage WHERE id=?').bind(`${post.reply}:1`).first()).toEqual({state:'succeeded',output_tokens:3});
 expect(await db.prepare('SELECT * FROM social_agent_replies WHERE id=?').bind(blocked.reply).first()).toEqual(before);expect(await db.prepare('SELECT state,attempt FROM social_agent_replies WHERE id=?').bind(absent.reply).first()).toEqual({state:'pending',attempt:0});expect(await db.prepare('SELECT count(*) n FROM social_comments WHERE post_id IN (?,?)').bind(blocked.id,absent.id).first('n')).toBe(0);
});
it('refuses Social provider output after Company withdrawal and leaves the claimed journal frozen until re-enrolled recovery',async()=>{
 const post=await social(f);await production();const provider=vi.fn(async()=>{await db.prepare('UPDATE engineering_verification_contexts SET enabled=false WHERE company_id=? AND run_id=?').bind(f.company,run).run();return reply();});
 await expect(drainSocialAgent(db,aiEnv,post.id,provider)).rejects.toThrow();expect(provider).toHaveBeenCalledTimes(1);expect(await db.prepare('SELECT count(*) n FROM social_comments WHERE post_id=?').bind(post.id).first('n')).toBe(0);expect(await db.prepare('SELECT state FROM social_agent_replies WHERE id=?').bind(post.reply).first('state')).toBe('running');
 const frozen=await db.prepare('SELECT * FROM social_agent_replies WHERE id=?').bind(post.reply).first();const retry=vi.fn();expect(await drainSocialAgent(db,aiEnv,post.id,retry)).toBe(false);expect(retry).not.toHaveBeenCalled();expect(await db.prepare('SELECT * FROM social_agent_replies WHERE id=?').bind(post.reply).first()).toEqual(frozen);
 await production();await db.prepare('UPDATE social_agent_replies SET lease_until=1 WHERE id=?').bind(post.reply).run();expect(await drainSocialAgent(db,aiEnv,post.id,retry)).toBe(false);expect(await db.prepare('SELECT state,error_code FROM social_agent_replies WHERE id=?').bind(post.reply).first()).toEqual({state:'failed',error_code:'AI_TIMEOUT'});expect(retry).not.toHaveBeenCalled();
});
it('keeps current Social IAM revocation effective after inference while enrolled Company execution remains available',async()=>{
 const post=await social(f);await production();const provider=vi.fn(async()=>{await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(Date.now(),f.owner.assignmentId).run();return reply();});await drainSocialAgent(db,aiEnv,post.id,provider);expect(provider).toHaveBeenCalledTimes(1);
 expect(await db.prepare('SELECT state,error_code FROM social_agent_replies WHERE id=?').bind(post.reply).first()).toEqual({state:'failed',error_code:'ACCESS_CHANGED'});expect(await db.prepare('SELECT count(*) n FROM social_comments WHERE post_id=?').bind(post.id).first('n')).toBe(0);
});
it('retains one reviewed Executive proposal without changing its native report and blocks disabled and unenrolled queued or expired jobs',async()=>{
 const own=await executive(f),blocked=await executive(disabled),absent=await executive(unenrolled),before=await reportView(db,f.owner,own.report);await db.prepare("UPDATE executive_generation_jobs SET state='running',lease_until=1 WHERE id=?").bind(blocked.job).run();const paused=await db.prepare('SELECT * FROM executive_generation_jobs WHERE id=?').bind(blocked.job).first();await production();const provider=vi.fn(async()=>proposal());
 await Promise.all([drainExecutiveGeneration(db,aiEnv,provider),drainExecutiveGeneration(db,aiEnv,provider)]);await drainExecutiveGeneration(db,aiEnv,provider);expect(provider).toHaveBeenCalledTimes(1);expect(await db.prepare('SELECT state FROM executive_generation_jobs WHERE id=?').bind(own.job).first('state')).toBe('ready');expect(await reportView(db,f.owner,own.report)).toEqual(before);
 expect(await db.prepare('SELECT * FROM executive_generation_jobs WHERE id=?').bind(blocked.job).first()).toEqual(paused);expect(await db.prepare('SELECT state,attempts FROM executive_generation_jobs WHERE id=?').bind(absent.job).first()).toEqual({state:'queued',attempts:0});expect(await db.prepare('SELECT count(*) n FROM company_release_acceptances').first('n')).toBe(0);
});
it('rejects an Executive proposal after Company withdrawal and preserves the saved report and its queued evidence',async()=>{
 const own=await executive(f),before=await reportView(db,f.owner,own.report);await production();const provider=vi.fn(async()=>{await db.prepare('UPDATE engineering_verification_contexts SET enabled=false WHERE company_id=? AND run_id=?').bind(f.company,run).run();return proposal();});await expect(drainExecutiveGeneration(db,aiEnv,provider)).rejects.toThrow();expect(provider).toHaveBeenCalledTimes(1);
 expect(await db.prepare('SELECT state,proposal_json FROM executive_generation_jobs WHERE id=?').bind(own.job).first()).toEqual({state:'running',proposal_json:null});expect(await reportView(db,f.owner,own.report)).toEqual(before);
});
it('keeps the ordinary compiled production boundary closed when the engineering attestation is absent',async()=>{
 const ticket=await supportTicket(f),post=await social(f),report=await executive(f);await production();vi.stubEnv('ERPX_ENGINEERING_VERIFICATION_RUN','');const provider=vi.fn();await drainSupportSla(db);expect(await drainSocialAgent(db,aiEnv,post.id,provider)).toBe(false);expect(await drainExecutiveGeneration(db,aiEnv,provider)).toBe(false);expect(provider).not.toHaveBeenCalled();
 expect(await db.prepare('SELECT count(*) n FROM support_sla_alerts WHERE ticket_id=?').bind(ticket).first('n')).toBe(0);expect(await db.prepare('SELECT state,attempt FROM social_agent_replies WHERE id=?').bind(post.reply).first()).toEqual({state:'pending',attempt:0});expect(await db.prepare('SELECT state,attempts FROM executive_generation_jobs WHERE id=?').bind(report.job).first()).toEqual({state:'queued',attempts:0});
});
