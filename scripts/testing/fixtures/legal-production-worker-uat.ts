/** Exercises an actual compiled jobs bundle in exact disposable Legal UAT databases. */
import { readFile, writeFile, open, realpath } from 'node:fs/promises';
import { resolve, join, dirname } from 'node:path';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { PostgresDatabase } from "../../../src/server/modules/runtime/adapters/runtime/postgres";
import { LocalFileStore } from "../../../src/server/modules/runtime/adapters/runtime/filesystem";
import { legalCommand, detail } from "../../../src/server/modules/legal/public/legal/service";
import { uploadDocument } from "../../../src/server/modules/legal/public/legal/documents";
import { queueInspection } from "../../../src/server/modules/legal/public/legal/processing";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import type { LegalUatUser } from "../../../fixtures/legal/builders/legal-browser-demo";
import {runLegalAiEngineeringFixture} from "../../../fixtures/legal/builders/legal-ai-engineering";
import type {BetaEnvironment} from "../../../src/server/modules/manage_erpx/public/beta-config";
import {pinnedNodeBinary,attachmentToolEnvironment} from "../runtime-tooling";
import {browserChildEnvironment} from "../runner-environment";

const root=resolve(import.meta.dir,'../../..'),run=process.argv[2],mode=process.argv[4]??'worker';
if(!run||!/^[a-f0-9]{12}$/.test(run)||!process.argv[3]||!['worker','ai'].includes(mode)||process.argv.length>5)throw new Error('EXACT_RUN_AND_COMPILED_RELEASE_REQUIRED');
const directory=join(root,'.runtime/browser',run),release=await realpath(resolve(process.argv[3]));
if(release!==join(directory,'release'))throw new Error('ATTESTED_ISOLATED_COMPILED_RELEASE_REQUIRED');
const artifact=JSON.parse(await readFile(join(release,'release.json'),'utf8'));
if(!/^release-[a-f0-9]{64}$/.test(artifact.softwareVersion))throw new Error('BUILD_SOFTWARE_VERSION_REQUIRED');
const state=JSON.parse(await readFile(join(directory,'state.json'),'utf8'));
if(state.run!==run||state.prefix!==`erpx_browser_${run}`||state.port!==3104||state.sourceFingerprint!==artifact.sourceFingerprint||process.env.ERPX_SERVER_SCRIPT_SOURCE_FINGERPRINT!==state.sourceFingerprint||!state.fixture?.legal)throw new Error('LEGAL_UAT_STATE_MISMATCH');
const pause=JSON.parse(await readFile(join(directory,'jobs-pause.json'),'utf8'));
if(state.jobsPaused!==true||state.jobsPid!=null||pause.scope!=='isolated-engineering'||pause.run!==run||pause.sourceFingerprint!==artifact.sourceFingerprint||pause.softwareVersion!==artifact.softwareVersion||pause.releaseManifestSha256!==artifact.fileManifestSha256||pause.stopped!==true||!Number.isInteger(pause.jobsPid))throw new Error('COHORT_JOBS_MUST_BE_EXACTLY_PAUSED');
try{process.kill(pause.jobsPid,0);throw new Error('COHORT_JOBS_STILL_RUNNING');}catch(error){if((error as NodeJS.ErrnoException).code!=='ESRCH')throw error;}
const manifest=JSON.parse(await readFile(join(directory,'databases.json'),'utf8'));
const requiredBindings=['CONTROL_DB','CONTROL_READ_DB','H001_DB','H001_READ_DB','H002_DB','H002_READ_DB','H003_DB','H003_READ_DB'];
if(Object.keys(manifest.databases).length!==8||!requiredBindings.every(binding=>Boolean(manifest.databases[binding]))||manifest.slots?.length!==3||new Set(manifest.slots.map((slot:{id:string})=>slot.id)).size!==3||manifest.slots.some((slot:{id:string;write:string;read:string;files:string})=>!/^h00[1-3]$/.test(slot.id)||slot.write!==`${slot.id.toUpperCase()}_DB`||slot.read!==`${slot.id.toUpperCase()}_READ_DB`||slot.files!==`${slot.id.toUpperCase()}_FILES`))throw new Error('EXACT_EIGHT_UAT_DATABASES_REQUIRED');
for(const [binding,value] of Object.entries(manifest.databases)){
  const url=new URL(String(value));
  if(!requiredBindings.includes(binding)||!['postgres:','postgresql:'].includes(url.protocol)||url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||url.pathname!==`/erpx_browser_${run}_${binding.toLowerCase()}`)throw new Error('EXACT_UAT_DATABASES_REQUIRED');
}
const credentials=JSON.parse(await readFile(join(directory,'legal-credentials.json'),'utf8')) as {users:LegalUatUser[]};
const guide=JSON.parse(await readFile(join(directory,'legal-guide.json'),'utf8'));
const {secret}=JSON.parse(await readFile(join(directory,'runtime.json'),'utf8'));
const node=pinnedNodeBinary(root);
if(Bun.version!=='1.4.3'||Bun.spawnSync([node,'--version']).stdout.toString().trim()!=='v26.11.1')throw new Error('PINNED_TOOLCHAIN_REQUIRED');
const h001=new PostgresDatabase({connectionString:manifest.databases.H001_DB,max:2});
const h002=new PostgresDatabase({connectionString:manifest.databases.H002_DB,max:2});
const files=new LocalFileStore(join(directory,'files/h001'),h001);
const key=()=>crypto.randomUUID();
const actor=(user:LegalUatUser):WorkContext=>({userId:user.userId,assignmentId:user.assignmentId,role:user.role,companyId:user.companyId,branchId:null,departmentId:null,holdingId:'h001',holdingSlug:'simulation',holdingName:'Synthetic Legal UAT',name:user.name,email:user.email,modules:['legal'],assignments:[],workspaces:[],inherited:false,assignmentScope:'holding',scope:{kind:'holding',holdingId:'h001',holdingSlug:'simulation',holdingName:'Synthetic Legal UAT',companyId:user.companyId}});
const manager=actor(credentials.users.find(user=>user.key==='A:manager')!);
const paused=actor(credentials.users.find(user=>user.key==='B:manager')!),escalationReviewer=credentials.users.find(user=>user.key==='A:reviewer-1')!;
const environment:NodeJS.ProcessEnv={PATH:`${dirname(node)}:${process.env.PATH??''}`,HOME:process.env.HOME??'',TMPDIR:process.env.TMPDIR??'/tmp',NODE_ENV:'production',ERPX_PROCESS_ROLE:'jobs',ERPX_ENVIRONMENT:'local',PORT:'3104',ERPX_BROWSER_SIMULATION_PORT:'3104',ERPX_ENGINEERING_VERIFICATION_RUN:run,ERPX_ENGINEERING_SOURCE_FINGERPRINT:artifact.sourceFingerprint,ERPX_PUBLIC_ORIGIN:'http://127.0.0.1:3104',BETTER_AUTH_URL:'http://127.0.0.1:3104',BETTER_AUTH_SECRET:secret,ERPX_DATA_DIR:directory,ERPX_DATABASE_MANIFEST:join(directory,'databases.json'),ERPX_PUBLIC_BETA:'false',ERPX_EMAIL_ADAPTER:'sandbox',ERPX_ATTACHMENTS_V2:'true',ERPX_ATTACHMENT_UPLOAD_FREEZE:'false',...attachmentToolEnvironment(root),NEXT_TELEMETRY_DISABLED:'1'};
// Source commands prepare fixtures; the compiled bundle enforces the attested engineering gate.
const nativeEnvironment=browserChildEnvironment(process.env,{...environment,NODE_ENV:'development',ERPX_ENVIRONMENT:'local'});
for(const name of Object.keys(process.env))if(!(name in nativeEnvironment))delete process.env[name];
Object.assign(process.env,nativeEnvironment);
const notice='Isolated engineering only. Synthetic identities and measured worker behavior; Company/provider certification remains pending.';
const attestation=JSON.parse(await readFile(join(directory,'engineering-attestation.json'),'utf8'));
const manifestBytes=await readFile(join(release,'release-files.json'));
if(attestation.scope!=='isolated-engineering'||attestation.run!==run||attestation.softwareVersion!==artifact.softwareVersion||attestation.sourceFingerprint!==artifact.sourceFingerprint||attestation.fileManifestSha256!==artifact.fileManifestSha256||createHash('sha256').update(manifestBytes).digest('hex')!==artifact.fileManifestSha256)throw new Error('WORKER_ENGINEERING_ATTESTATION_MISMATCH');
const engineering=await h001.prepare('SELECT company_id,enabled,expires_at FROM engineering_verification_contexts WHERE run_id=? AND source_fingerprint=? AND software_version=? AND release_manifest_sha256=?').bind(run,artifact.sourceFingerprint,artifact.softwareVersion,artifact.fileManifestSha256).all<{company_id:string;enabled:boolean;expires_at:number}>();
if(!engineering.results.some(row=>row.company_id===manager.companyId&&row.enabled&&row.expires_at>Date.now())||!engineering.results.some(row=>row.company_id===paused.companyId&&!row.enabled))throw new Error('WORKER_COMPANY_ENROLLMENT_MISMATCH');
const certificationSnapshot=async()=>JSON.stringify((await h001.prepare(`SELECT 'business' kind,row_to_json(b)::text evidence FROM company_business_acceptance b WHERE company_id IN (?,?)
 UNION ALL SELECT 'release',row_to_json(a)::text FROM company_release_acceptances a WHERE company_id IN (?,?)
 UNION ALL SELECT 'review',row_to_json(r)::text FROM company_release_gate_reviews r WHERE company_id IN (?,?) ORDER BY kind,evidence`).bind(manager.companyId,paused.companyId,manager.companyId,paused.companyId,manager.companyId,paused.companyId).all()).results);
const certificationBefore=await certificationSnapshot();
let worker:ReturnType<typeof spawn>|undefined;
let interrupted:string|undefined;
for(const signal of ['SIGINT','SIGTERM'] as const)process.on(signal,()=>{interrupted=signal;worker?.kill('SIGTERM');});
const workerPids:number[]=[];
async function stopWorker(){
  if(!worker||worker.exitCode!==null)return;
  const current=worker;worker=undefined;
  await new Promise<void>(resolve=>{current.once('exit',()=>resolve());current.kill('SIGTERM');setTimeout(()=>{if(current.exitCode===null)current.kill('SIGKILL');},5000).unref();});
}
async function startWorker(){
  if(interrupted)throw new Error(`LEGAL_WORKER_PREREQUISITE_INTERRUPTED:${interrupted}`);
  const log=await open(join(directory,'legal-production-worker.log'),'a',0o600);
  worker=spawn(node,[join(release,'tools/jobs.mjs')],{cwd:release,env:environment,stdio:['ignore',log.fd,log.fd]});
  await log.close();if(!worker.pid)throw new Error('COMPILED_WORKER_START_FAILED');workerPids.push(worker.pid);
}
async function until(check:()=>Promise<boolean>,timeout=45_000){
  const end=Date.now()+timeout;
  while(Date.now()<end){if(interrupted)throw new Error(`LEGAL_WORKER_PREREQUISITE_INTERRUPTED:${interrupted}`);if(await check())return;if(worker&&worker.exitCode!==null)throw new Error('COMPILED_WORKER_EXITED');await Bun.sleep(500);}
  throw new Error('COMPILED_WORKER_TIMEOUT');
}
try{
  const ai=await runLegalAiEngineeringFixture(h001,environment as BetaEnvironment,manager,actor(escalationReviewer));
  if(interrupted)throw new Error(`LEGAL_WORKER_PREREQUISITE_INTERRUPTED:${interrupted}`);
  await writeFile(join(directory,'legal-ai-engineering-evidence.json'),JSON.stringify({...ai,run,sourceFingerprint:artifact.sourceFingerprint,softwareVersion:artifact.softwareVersion,releaseManifestSha256:artifact.fileManifestSha256},null,2)+'\n',{mode:0o600});
  if(mode==='ai')console.log(JSON.stringify({run,evidencePath:join(directory,'legal-ai-engineering-evidence.json'),provider:'declared deterministic adapter; no network call'}));
  else{
  const makeDue=async(who:WorkContext)=>{
    const created=await legalCommand(h001,who,{action:'create',item:{kind:'obligation',title:`Compiled worker ${who===manager?'enabled':'paused'} ${run}`,ownerId:who.userId,dueDate:'2026-01-01',data:{obligationType:'Synthetic worker reminder',requirement:notice,escalationOwner:who===manager?escalationReviewer.userId:who.userId}}},key());
    return String(created.id);
  };
  const enabledItem=await makeDue(manager),pausedItem=await makeDue(paused);
  await legalCommand(h001,manager,{action:'member',id:enabledItem,expectedVersion:(await detail(h001,manager,enabledItem)).item.version,userId:escalationReviewer.userId,access:'read'},key());
  const inspectionItem=await legalCommand(h001,manager,{action:'create',item:{kind:'matter',title:`Compiled worker OCR ${run}`,ownerId:manager.userId,data:{parties:'Synthetic training parties'}}},key());
  const bytes=new Uint8Array(await readFile(guide.signedPdfPath));
  const uploaded=await uploadDocument(h001,files,manager,String(inspectionItem.id),1,'Worker OCR original.pdf','application/pdf',bytes,key());
  const documents=(await detail(h001,manager,String(inspectionItem.id))).documents;
  const job=await queueInspection(h001,manager,documents[0].id,'ocr',key());
  // A statement failure in the other Holding must not stop the enabled Holding.
  const failureBaseline=await h002.prepare("SELECT heartbeat_at,failure_count FROM legal_runtime_health WHERE task='documents'").first<{heartbeat_at:number;failure_count:number}>();
  await h002.exec(`CREATE FUNCTION legal_uat_processing_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'LEGAL_UAT_PROCESSING_FAILURE'; END $$;
    CREATE TRIGGER legal_uat_processing_failure BEFORE UPDATE ON legal_processing_jobs FOR EACH STATEMENT EXECUTE FUNCTION legal_uat_processing_failure();`);
  const controlFixturePath=join(directory,'legal-production-worker-control-fixture.json');
  const controlFixture=JSON.stringify({run,softwareVersion:artifact.softwareVersion,sourceFingerprint:artifact.sourceFingerprint,enabledCompany:manager.companyId,pausedCompany:paused.companyId,recordIds:{enabledItem,pausedItem,inspectionItem:inspectionItem.id,documentId:documents[0].id,jobId:job.jobId},notice,purpose:'Attested engineering controls; no Company release acceptance or provider certification is changed.'},null,2)+'\n';
  await writeFile(controlFixturePath,controlFixture,{mode:0o600});
  const controlFixtureSha256=createHash('sha256').update(controlFixture).digest('hex');
  await startWorker();
  await until(async()=>{
    const notified=await h001.prepare('SELECT count(*) n FROM legal_reminders WHERE item_id=?').bind(enabledItem).first<{n:number}>();
    const processing=await h001.prepare('SELECT state FROM legal_processing_jobs WHERE id=?').bind(job.jobId).first<{state:string}>();
    return Number(notified?.n)===1&&processing?.state==='completed';
  });
  await until(async()=>{
    const failure=await h002.prepare("SELECT heartbeat_at,failure_count,last_error FROM legal_runtime_health WHERE task='documents'").first<{heartbeat_at:number;failure_count:number;last_error:string}>();
    return Number(failure?.failure_count??0)>Number(failureBaseline?.failure_count??0)&&Number(failure?.heartbeat_at??0)>Number(failureBaseline?.heartbeat_at??0)&&failure?.last_error==='P0001';
  });
  const failure=await h002.prepare("SELECT heartbeat_at,failure_count,last_error FROM legal_runtime_health WHERE task='documents'").first<{heartbeat_at:number;failure_count:number;last_error:string}>();
  if(!failure||failure.failure_count<1||Number(failure.failure_count)<=Number(failureBaseline?.failure_count??0)||Number(failure.heartbeat_at)<=Number(failureBaseline?.heartbeat_at??0)||failure.last_error!=='P0001')throw new Error('HOLDING_FAILURE_NOT_OBSERVABLE');
  if((await h001.prepare('SELECT count(*) n FROM legal_reminders WHERE item_id=?').bind(pausedItem).first<{n:number}>())?.n!==0)throw new Error('PAUSED_COMPANY_SIDE_EFFECT');
  const notificationCounts=async()=>({owner:Number(await h001.prepare("SELECT count(*) n FROM staff_notifications WHERE record_type='legal' AND record_id=? AND user_id=? AND message='Legal deadline requires attention'").bind(enabledItem,manager.userId).first('n')),escalation:Number(await h001.prepare("SELECT count(*) n FROM staff_notifications WHERE record_type='legal' AND record_id=? AND user_id=? AND message='Overdue legal commitment escalated to you'").bind(enabledItem,escalationReviewer.userId).first('n'))});
  await until(async()=>{const counts=await notificationCounts();return counts.owner===1&&counts.escalation===1;});
  await stopWorker();
  const preRestartHeartbeat=Number((await h001.prepare("SELECT heartbeat_at FROM legal_runtime_health WHERE task='reminders'").first<{heartbeat_at:number}>())?.heartbeat_at??0);
  if(preRestartHeartbeat<=0)throw new Error('PRE_RESTART_HEARTBEAT_REQUIRED');
  await startWorker();
  let postRestartHeartbeat=0;
  await until(async()=>{
    postRestartHeartbeat=Number((await h001.prepare("SELECT heartbeat_at FROM legal_runtime_health WHERE task='reminders'").first<{heartbeat_at:number}>())?.heartbeat_at??0);
    return postRestartHeartbeat>preRestartHeartbeat;
  });
  if(Number((await h001.prepare('SELECT count(*) n FROM legal_reminders WHERE item_id=?').bind(enabledItem).first<{n:number}>())?.n)!==1)throw new Error('DUPLICATE_REMINDER_AFTER_RESTART');
  const notifications=await notificationCounts();if(notifications.owner!==1||notifications.escalation!==1)throw new Error('WORKER_RECIPIENT_OR_ESCALATION_DUPLICATED');
  if(await certificationSnapshot()!==certificationBefore)throw new Error('ENGINEERING_WORKER_CHANGED_CERTIFICATION');
  const inspected=await h001.prepare('SELECT d.checksum,j.result_json FROM legal_processing_jobs j JOIN legal_documents d ON d.id=j.document_id WHERE j.id=?').bind(job.jobId).first<{checksum:string;result_json:string}>();
  const extracted=inspected?JSON.parse(inspected.result_json) as {text?:string;checksum?:string}:null;
  if(!inspected||inspected.checksum!==createHash('sha256').update(bytes).digest('hex')||extracted?.checksum!==inspected.checksum||!extracted?.text?.trim())throw new Error('WORKER_ORIGINAL_OR_EXTRACTED_TEXT_MISMATCH');
  const result={scope:'isolated-engineering',run,sourceFingerprint:artifact.sourceFingerprint,releaseManifestSha256:artifact.fileManifestSha256,softwareVersion:artifact.softwareVersion,enabledCompany:manager.companyId,pausedCompany:paused.companyId,recordIds:{enabledItem,pausedItem,inspectionItem:inspectionItem.id,documentId:documents[0].id,jobId:job.jobId},passed:['compiled_reminder','compiled_ocr_original_checksum','paused_company','restart_deduplication','holding_failure_heartbeat','certification_unchanged'],controlFixture:{path:controlFixturePath,sha256:controlFixtureSha256},original:{sha256:inspected.checksum,extractedTextLength:extracted.text.length},notifications:{...notifications,ownerId:manager.userId,escalationOwnerId:escalationReviewer.userId},restart:{preRestartHeartbeat,postRestartHeartbeat,cohortJobsPaused:true,cohortJobsPid:pause.jobsPid,workerPids,firstWorkerPid:workerPids[0],restartedWorkerPid:workerPids[1]},notice,completedAt:new Date().toISOString(),uploadedVersion:uploaded.version};
  await writeFile(join(directory,'legal-production-worker-evidence.json'),JSON.stringify(result,null,2)+'\n',{mode:0o600});
  console.log(JSON.stringify({run,passed:result.passed,evidencePath:join(directory,'legal-production-worker-evidence.json'),notice}));
  }
}finally{
  await stopWorker();
  await h002.exec('DROP TRIGGER IF EXISTS legal_uat_processing_failure ON legal_processing_jobs; DROP FUNCTION IF EXISTS legal_uat_processing_failure();').catch(()=>{});
  await Promise.all([h001.close(),h002.close()]);
}
