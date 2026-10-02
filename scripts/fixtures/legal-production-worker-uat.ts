/** Exercises an actual compiled jobs bundle in exact disposable Legal UAT databases. */
import { readFile, writeFile, open, realpath } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { PostgresDatabase } from '../../src/server/runtime/postgres';
import { LocalFileStore } from '../../src/server/runtime/filesystem';
import { legalReleaseCriteria, legalProductionScope } from '../../src/data/production-scope';
import { legalCommand, detail } from '../../src/server/legal/service';
import { uploadDocument } from '../../src/server/legal/documents';
import { queueInspection } from '../../src/server/legal/processing';
import type { WorkContext } from '../../src/server/authorization';
import type { LegalUatUser } from './legal-browser-demo';

const root=resolve(import.meta.dir,'../..'),run=process.argv[2];
if(!run||!/^[a-f0-9]{12}$/.test(run)||!process.argv[3])throw new Error('EXACT_RUN_AND_COMPILED_RELEASE_REQUIRED');
const directory=join(root,'.runtime/browser',run),release=await realpath(resolve(process.argv[3]));
if(!release.startsWith(join(root,'.runtime/releases')+'/'))throw new Error('ISOLATED_COMPILED_RELEASE_REQUIRED');
const artifact=JSON.parse(await readFile(join(release,'release.json'),'utf8'));
if(!/^release-[a-f0-9]{64}$/.test(artifact.softwareVersion))throw new Error('BUILD_SOFTWARE_VERSION_REQUIRED');
const state=JSON.parse(await readFile(join(directory,'state.json'),'utf8'));
if(state.run!==run||state.prefix!==`erpx_browser_${run}`||state.port!==3104||!state.fixture?.legal)throw new Error('LEGAL_UAT_STATE_MISMATCH');
const manifest=JSON.parse(await readFile(join(directory,'databases.json'),'utf8'));
for(const [binding,value] of Object.entries(manifest.databases)){
  const url=new URL(String(value));
  if(url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||url.pathname!==`/erpx_browser_${run}_${binding.toLowerCase()}`)throw new Error('EXACT_UAT_DATABASES_REQUIRED');
}
const credentials=JSON.parse(await readFile(join(directory,'legal-credentials.json'),'utf8')) as {users:LegalUatUser[]};
const guide=JSON.parse(await readFile(join(directory,'legal-guide.json'),'utf8'));
const {secret}=JSON.parse(await readFile(join(directory,'runtime.json'),'utf8'));
const node=join(root,'.runtime/node/node-v24.20.0-darwin-arm64/bin/node');
if(Bun.version!=='1.2.17'||Bun.spawnSync([node,'--version']).stdout.toString().trim()!=='v24.20.0')throw new Error('PINNED_TOOLCHAIN_REQUIRED');
const h001=new PostgresDatabase({connectionString:manifest.databases.H001_DB,max:2});
const h002=new PostgresDatabase({connectionString:manifest.databases.H002_DB,max:2});
const files=new LocalFileStore(join(directory,'files/h001'),h001);
const key=()=>crypto.randomUUID();
const actor=(user:LegalUatUser):WorkContext=>({userId:user.userId,assignmentId:user.assignmentId,role:user.role,companyId:user.companyId,branchId:null,departmentId:null,holdingId:'h001',holdingSlug:'simulation',holdingName:'Synthetic Legal UAT',name:user.name,email:user.email,modules:['legal'],assignments:[],workspaces:[],inherited:false,assignmentScope:'holding',scope:{kind:'holding',holdingId:'h001',holdingSlug:'simulation',holdingName:'Synthetic Legal UAT',companyId:user.companyId}});
const manager=actor(credentials.users.find(user=>user.key==='A:manager')!),reviewer=credentials.users.find(user=>user.key==='A:owner')!;
const paused=actor(credentials.users.find(user=>user.key==='B:manager')!);
const environment:NodeJS.ProcessEnv={PATH:`${join(root,'.runtime/node/node-v24.20.0-darwin-arm64/bin')}:${process.env.PATH??''}`,HOME:process.env.HOME??'',TMPDIR:process.env.TMPDIR??'/tmp',NODE_ENV:'production',ERPX_PROCESS_ROLE:'jobs',ERPX_ENVIRONMENT:'production',ERPX_PUBLIC_ORIGIN:'https://legal-uat.invalid',BETTER_AUTH_URL:'https://legal-uat.invalid',BETTER_AUTH_SECRET:secret,ERPX_DATA_DIR:directory,ERPX_DATABASE_MANIFEST:join(directory,'databases.json'),ERPX_PUBLIC_BETA:'false',ERPX_EMAIL_ADAPTER:'sandbox',ERPX_ATTACHMENTS_V2:'true',ERPX_CLAMD_SOCKET:join(root,'.scratch/dev/clamav/clamd.sock'),ERPX_CLAMAV_DATABASE_DIR:join(root,'.scratch/dev/clamav/db'),ERPX_CLAMAV_VERSION:'1.5.4',ERPX_HEIF_CONVERT:'/opt/homebrew/opt/libheif/bin/heif-convert',ERPX_PDFTOPPM_BIN:'/opt/homebrew/bin/pdftoppm',NEXT_TELEMETRY_DISABLED:'1'};
// Source commands prepare synthetic jobs; the launched bundle enforces production gates.
Object.assign(process.env,{...environment,NODE_ENV:'development',ERPX_ENVIRONMENT:'local'});
const releaseId=`legal-worker-uat-${key()}`,notice='UAT controls only. Synthetic identities and expected machine behavior; no expert legal certification.';
let worker:ReturnType<typeof spawn>|undefined;
async function stopWorker(){
  if(!worker||worker.exitCode!==null)return;
  const current=worker;worker=undefined;
  await new Promise<void>(resolve=>{current.once('exit',()=>resolve());current.kill('SIGTERM');setTimeout(()=>{if(current.exitCode===null)current.kill('SIGKILL');},5000).unref();});
}
async function startWorker(){
  const log=await open(join(directory,'legal-production-worker.log'),'a',0o600);
  worker=spawn(node,[join(release,'tools/jobs.mjs')],{cwd:release,env:environment,stdio:['ignore',log.fd,log.fd]});
  await log.close();if(!worker.pid)throw new Error('COMPILED_WORKER_START_FAILED');
}
async function until(check:()=>Promise<boolean>,timeout=45_000){
  const end=Date.now()+timeout;
  while(Date.now()<end){if(await check())return;if(worker&&worker.exitCode!==null)throw new Error('COMPILED_WORKER_EXITED');await Bun.sleep(500);}
  throw new Error('COMPILED_WORKER_TIMEOUT');
}
try{
  const makeDue=async(who:WorkContext)=>{
    const created=await legalCommand(h001,who,{action:'create',item:{kind:'obligation',title:`Compiled worker ${who===manager?'enabled':'paused'} ${run}`,ownerId:who.userId,dueDate:'2026-01-01',data:{obligationType:'Synthetic worker reminder',requirement:notice}}},key());
    return String(created.id);
  };
  const enabledItem=await makeDue(manager),pausedItem=await makeDue(paused);
  const inspectionItem=await legalCommand(h001,manager,{action:'create',item:{kind:'matter',title:`Compiled worker OCR ${run}`,ownerId:manager.userId,data:{parties:'Synthetic training parties'}}},key());
  const bytes=new Uint8Array(await readFile(guide.signedPdfPath));
  const uploaded=await uploadDocument(h001,files,manager,String(inspectionItem.id),1,'Worker OCR original.pdf','application/pdf',bytes,key());
  const documents=(await detail(h001,manager,String(inspectionItem.id))).documents;
  const job=await queueInspection(h001,manager,documents[0].id,'ocr',key());
  // A statement failure in the other Holding must not stop the enabled Holding.
  await h002.exec(`CREATE FUNCTION legal_uat_processing_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'LEGAL_UAT_PROCESSING_FAILURE'; END $$;
    CREATE TRIGGER legal_uat_processing_failure BEFORE UPDATE ON legal_processing_jobs FOR EACH STATEMENT EXECUTE FUNCTION legal_uat_processing_failure();`);
  await h001.prepare(`INSERT INTO company_business_acceptance(company_id,status,version,requested_by,reviewed_by,requested_at,reviewed_at,reason,review_reason,updated_at)
    VALUES(?,'accepted',1,?,?,1,1,?,?,1) ON CONFLICT(company_id) DO NOTHING`).bind(manager.companyId,manager.userId,reviewer.userId,notice,notice).run();
  const revision=Number((await h001.prepare('SELECT revision FROM company_legal_release_revisions WHERE company_id=?').bind(manager.companyId).first<{revision:number}>())?.revision??0);
  const setup=Number((await h001.prepare('SELECT version FROM company_business_acceptance WHERE company_id=?').bind(manager.companyId).first<{version:number}>())!.version);
  const controlFixturePath=join(directory,'legal-production-worker-control-fixture.json');
  const controlFixture=JSON.stringify({
    run,scopeVersion:legalProductionScope.version,softwareVersion:artifact.softwareVersion,
    enabledCompany:manager.companyId,pausedCompany:paused.companyId,
    recordIds:{enabledItem,pausedItem,inspectionItem:inspectionItem.id,documentId:documents[0].id,jobId:job.jobId},
    criteria:legalReleaseCriteria.map(criterion=>criterion.id),notice,
    purpose:'Synthetic controls to exercise the production worker; not production release acceptance evidence.',
  },null,2)+'\n';
  await writeFile(controlFixturePath,controlFixture,{mode:0o600});
  const controlFixtureSha256=createHash('sha256').update(controlFixture).digest('hex');
  const evidence=JSON.stringify(legalReleaseCriteria.map(criterion=>({criterionId:criterion.id,summary:notice,files:[{reference:controlFixturePath,sha256:controlFixtureSha256}]})));
  const evidenceSha256=createHash('sha256').update(evidence).digest('hex');
  const now=Date.now();
  await h001.batch([
    h001.prepare(`INSERT INTO company_release_acceptances(id,company_id,scope_version,software_version,configuration_revision,setup_version,evidence_sha256,evidence_json,requested_by,reason,created_at,updated_at)
      VALUES(?,?,?,?,?,?,?, ?,?,?,?,?)`).bind(releaseId,manager.companyId,legalProductionScope.version,artifact.softwareVersion,revision,setup,evidenceSha256,evidence,manager.userId,notice,now,now),
    ...legalReleaseCriteria.map(criterion=>h001.prepare("INSERT INTO company_release_gate_reviews(company_id,release_id,criterion_id,state,reviewer_id,assignment_id,permission,reason,reviewed_at) VALUES(?,?,?,'passed',?,?,?,?,?)").bind(manager.companyId,releaseId,criterion.id,reviewer.userId,reviewer.assignmentId,criterion.permission,notice,now)),
    h001.prepare("INSERT INTO company_feature_rollouts(company_id,module_id,rollout_state,version,reason,changed_by,changed_at) VALUES(?,'legal','available',1,?,?,?) ON CONFLICT(company_id,module_id) DO UPDATE SET rollout_state='available',version=company_feature_rollouts.version+1,reason=excluded.reason,changed_by=excluded.changed_by,changed_at=excluded.changed_at").bind(manager.companyId,notice,manager.userId,now),
  ]);
  await startWorker();
  await until(async()=>{
    const notified=await h001.prepare('SELECT count(*) n FROM legal_reminders WHERE item_id=?').bind(enabledItem).first<{n:number}>();
    const processing=await h001.prepare('SELECT state FROM legal_processing_jobs WHERE id=?').bind(job.jobId).first<{state:string}>();
    return Number(notified?.n)===1&&processing?.state==='completed';
  });
  const failure=await h002.prepare("SELECT failure_count,last_error FROM legal_runtime_health WHERE task='documents'").first<{failure_count:number;last_error:string}>();
  if(!failure||failure.failure_count<1)throw new Error('HOLDING_FAILURE_NOT_OBSERVABLE');
  if((await h001.prepare('SELECT count(*) n FROM legal_reminders WHERE item_id=?').bind(pausedItem).first<{n:number}>())?.n!==0)throw new Error('PAUSED_COMPANY_SIDE_EFFECT');
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
  const result={run,softwareVersion:artifact.softwareVersion,enabledCompany:manager.companyId,pausedCompany:paused.companyId,recordIds:{enabledItem,pausedItem,inspectionItem:inspectionItem.id,documentId:documents[0].id,jobId:job.jobId},passed:['production_reminder','production_ocr','paused_company','restart_deduplication','holding_failure_heartbeat'],controlFixture:{path:controlFixturePath,sha256:controlFixtureSha256,evidenceSha256},restart:{preRestartHeartbeat,postRestartHeartbeat},notice,completedAt:new Date().toISOString(),uploadedVersion:uploaded.version};
  await writeFile(join(directory,'legal-production-worker-evidence.json'),JSON.stringify(result,null,2)+'\n',{mode:0o600});
  console.log(JSON.stringify({run,passed:result.passed,evidencePath:join(directory,'legal-production-worker-evidence.json'),notice}));
}finally{
  await stopWorker();
  await h001.prepare('UPDATE company_release_acceptances SET revoked_at=?,version=version+1,updated_at=? WHERE id=? AND revoked_at IS NULL').bind(Date.now(),Date.now(),releaseId).run().catch(()=>{});
  await h001.prepare("UPDATE company_feature_rollouts SET rollout_state='off',version=version+1 WHERE company_id=? AND module_id='legal'").bind(manager.companyId).run().catch(()=>{});
  await h002.exec('DROP TRIGGER IF EXISTS legal_uat_processing_failure ON legal_processing_jobs; DROP FUNCTION IF EXISTS legal_uat_processing_failure();').catch(()=>{});
  await Promise.all([h001.close(),h002.close()]);
}
