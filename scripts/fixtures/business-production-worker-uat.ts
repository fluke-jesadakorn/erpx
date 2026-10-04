/** Actual packaged production jobs, strict paused disposable cohort, and native Business sources. */
import {readFile,writeFile,open,realpath} from 'node:fs/promises';
import {resolve,join} from 'node:path';
import {spawn} from 'node:child_process';
import {createHash} from 'node:crypto';
import {PostgresDatabase} from '../../src/server/runtime/postgres';
import {LocalFileStore} from '../../src/server/runtime/filesystem';
import {closeRuntime} from '../../src/server/runtime/env';
import {enrollEngineeringVerification} from '../../src/server/engineering-verification';
import {seedBusinessWorkerSources} from './business-worker-sources';
import {runtimeSourceFingerprint} from '../local/source-fingerprint';

const root=resolve(import.meta.dir,'../..'),run=process.argv[2];
if(!run||!/^[a-f0-9]{12}$/.test(run)||!process.argv[3])throw new Error('EXACT_RUN_AND_COMPILED_RELEASE_REQUIRED');
const directory=join(root,'.runtime/browser',run),release=await realpath(resolve(process.argv[3])),node=join(root,'.runtime/node/node-v24.20.0-darwin-arm64/bin/node');
if(release!==join(directory,'release'))throw new Error('ATTESTED_ISOLATED_COMPILED_RELEASE_REQUIRED');
if(Bun.version!=='1.2.17'||Bun.spawnSync([node,'--version']).stdout.toString().trim()!=='v24.20.0')throw new Error('PINNED_TOOLCHAIN_REQUIRED');
const artifact=JSON.parse(await readFile(join(release,'release.json'),'utf8'));
if(!/^[a-f0-9]{64}$/.test(artifact.sourceFingerprint)||artifact.softwareVersion!==`release-${artifact.sourceFingerprint}`||artifact.node!=='24.20.0')throw new Error('BUILD_SOFTWARE_VERSION_REQUIRED');
const state=JSON.parse(await readFile(join(directory,'state.json'),'utf8'));
if(state.run!==run||state.prefix!==`erpx_browser_${run}`||state.port!==3104||state.ready!==true||state.diagnostic===true||state.sourceFingerprint!==artifact.sourceFingerprint||state.softwareVersion!==artifact.softwareVersion||state.release?.fileManifestSha256!==artifact.fileManifestSha256||state.webEntrypoint!==join(release,'server.js')||state.jobsEntrypoint!==join(release,'tools/jobs.mjs')||!Number.isInteger(state.pid))throw new Error('BUSINESS_COMPILED_COHORT_STATE_MISMATCH');
const webCommand=Bun.spawnSync(['ps','-p',String(state.pid),'-o','command=']).stdout.toString().trim(),webCwd=Bun.spawnSync(['lsof','-a','-p',String(state.pid),'-d','cwd','-Fn']).stdout.toString().split('\n').find(line=>line.startsWith('n'))?.slice(1);
const listener=Bun.spawnSync(['lsof','-nP','-a','-p',String(state.pid),'-iTCP:3104','-sTCP:LISTEN','-Fn']).stdout.toString();
const ownedNextTitle=/^next-server(?:\s|$)/.test(webCommand);
if((!webCommand.includes(join(release,'server.js'))&&!ownedNextTitle)||webCwd!==release||!listener.includes(':3104'))throw new Error('EXACT_3104_COMPILED_WEB_OWNERSHIP_REQUIRED');
const pause=JSON.parse(await readFile(join(directory,'jobs-pause.json'),'utf8'));
if(state.jobsPaused!==true||state.jobsPid!=null||pause.scope!=='isolated-engineering'||pause.run!==run||pause.sourceFingerprint!==artifact.sourceFingerprint||pause.softwareVersion!==artifact.softwareVersion||pause.releaseManifestSha256!==artifact.fileManifestSha256||pause.stopped!==true||!Number.isInteger(pause.jobsPid)||pause.entrypoint!==join(release,'tools/jobs.mjs')||pause.cwd!==release)throw new Error('COHORT_JOBS_MUST_BE_EXACTLY_PAUSED');
try{process.kill(pause.jobsPid,0);throw new Error('COHORT_JOBS_STILL_RUNNING');}catch(error){if((error as NodeJS.ErrnoException).code!=='ESRCH')throw error;}
const manifest=JSON.parse(await readFile(join(directory,'databases.json'),'utf8'));
const bindings=['CONTROL_DB','CONTROL_READ_DB','H001_DB','H001_READ_DB','H002_DB','H002_READ_DB','H003_DB','H003_READ_DB'];
if(Object.keys(manifest.databases).length!==8||!bindings.every(binding=>Boolean(manifest.databases[binding]))||manifest.slots?.length!==3||new Set(manifest.slots.map((slot:{id:string})=>slot.id)).size!==3||manifest.slots.some((slot:{id:string;write:string;read:string;files:string})=>!/^h00[1-3]$/.test(slot.id)||slot.write!==`${slot.id.toUpperCase()}_DB`||slot.read!==`${slot.id.toUpperCase()}_READ_DB`||slot.files!==`${slot.id.toUpperCase()}_FILES`))throw new Error('EXACT_EIGHT_UAT_DATABASES_REQUIRED');
for(const [binding,value] of Object.entries(manifest.databases)){
 const url=new URL(String(value));if(!bindings.includes(binding)||!['postgres:','postgresql:'].includes(url.protocol)||url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||url.pathname!==`/erpx_browser_${run}_${binding.toLowerCase()}`)throw new Error('EXACT_UAT_DATABASES_REQUIRED');
}
const attestation=JSON.parse(await readFile(join(directory,'engineering-attestation.json'),'utf8')),manifestBytes=await readFile(join(release,'release-files.json'));
if(attestation.scope!=='isolated-engineering'||attestation.run!==run||attestation.sourceFingerprint!==artifact.sourceFingerprint||attestation.softwareVersion!==artifact.softwareVersion||attestation.fileManifestSha256!==artifact.fileManifestSha256||createHash('sha256').update(manifestBytes).digest('hex')!==artifact.fileManifestSha256||await runtimeSourceFingerprint(root)!==artifact.sourceFingerprint)throw new Error('WORKER_ENGINEERING_ATTESTATION_MISMATCH');
const checksums=JSON.parse(manifestBytes.toString()) as Record<string,string>;
for(const name of ['server.js','tools/jobs.mjs'])if(checksums[name]!==createHash('sha256').update(await readFile(join(release,name))).digest('hex'))throw new Error('PACKAGED_RUNTIME_CHECKSUM_MISMATCH');
const {secret}=JSON.parse(await readFile(join(directory,'runtime.json'),'utf8'));
const environment:NodeJS.ProcessEnv={PATH:`${join(root,'.runtime/node/node-v24.20.0-darwin-arm64/bin')}:${process.env.PATH??''}`,HOME:process.env.HOME??'',TMPDIR:process.env.TMPDIR??'/tmp',NODE_ENV:'production',ERPX_PROCESS_ROLE:'jobs',ERPX_ENVIRONMENT:'local',PORT:'3104',ERPX_BROWSER_SIMULATION_PORT:'3104',ERPX_ENGINEERING_VERIFICATION_RUN:run,ERPX_ENGINEERING_SOURCE_FINGERPRINT:artifact.sourceFingerprint,ERPX_PUBLIC_ORIGIN:'http://127.0.0.1:3104',BETTER_AUTH_URL:'http://127.0.0.1:3104',BETTER_AUTH_SECRET:secret,ERPX_DATA_DIR:directory,ERPX_DATABASE_MANIFEST:join(directory,'databases.json'),ERPX_PUBLIC_BETA:'false',ERPX_EMAIL_ADAPTER:'sandbox',ERPX_ATTACHMENTS_V2:'true',ERPX_ATTACHMENT_UPLOAD_FREEZE:'false',ERPX_CLAMD_SOCKET:join(root,'.scratch/dev/clamav/clamd.sock'),ERPX_CLAMAV_DATABASE_DIR:join(root,'.scratch/dev/clamav/db'),ERPX_CLAMAV_VERSION:'1.5.4',ERPX_HEIF_CONVERT:'/opt/homebrew/opt/libheif/bin/heif-convert',ERPX_PDFTOPPM_BIN:'/opt/homebrew/bin/pdftoppm',NEXT_TELEMETRY_DISABLED:'1'};
// Native source commands run under the declared fixture environment. Only packaged jobs enforce production gates.
Object.assign(process.env,{...environment,NODE_ENV:'development'});
const databases=Object.fromEntries(await Promise.all(bindings.map(async binding=>{const db=new PostgresDatabase({connectionString:manifest.databases[binding],max:2});const row=await db.prepare('SELECT current_database() name,current_user actor,pg_get_userbyid(datdba) owner FROM pg_database WHERE datname=current_database()').first<{name:string;actor:string;owner:string}>();if(row?.name!==`erpx_browser_${run}_${binding.toLowerCase()}`||row.actor!=='erpx_test_admin'||row.owner!=='erpx_test_admin'){await db.close();throw new Error('UAT_DATABASE_OWNER_CHANGED');}return [binding,db] as const;})));
const db=databases.H001_DB,control=databases.CONTROL_DB,files=new LocalFileStore(join(directory,'files/h001'),db),now=Date.now();
const names=['enabled','disabled','unenrolled','withdrawn','expired'] as const;
type Sources=Awaited<ReturnType<typeof seedBusinessWorkerSources>>;
let sources:Record<typeof names[number],Sources>;
let worker:ReturnType<typeof spawn>|undefined;
const workerPids:number[]=[],heartbeats:number[]=[];
async function stopWorker(){
 const current=worker;worker=undefined;if(!current||current.exitCode!==null)return;
 await new Promise<void>(resolve=>{current.once('exit',()=>resolve());current.kill('SIGTERM');setTimeout(()=>{if(current.exitCode===null)current.kill('SIGKILL');},5000).unref();});
}
async function startWorker(){
 const log=await open(join(directory,'business-production-worker.log'),'a',0o600);worker=spawn(node,[join(release,'tools/jobs.mjs')],{cwd:release,env:environment,stdio:['ignore',log.fd,log.fd]});await log.close();if(!worker.pid)throw new Error('COMPILED_WORKER_START_FAILED');workerPids.push(worker.pid);
}
async function heartbeat(){return Number(await control.prepare("SELECT heartbeat_at FROM runtime_health WHERE service='jobs'").first('heartbeat_at')??0);}
async function until(check:()=>Promise<boolean>,timeout=60_000){const end=Date.now()+timeout;while(Date.now()<end){if(await check())return;if(worker&&worker.exitCode!==null)throw new Error('COMPILED_WORKER_EXITED');await Bun.sleep(500);}throw new Error('COMPILED_BUSINESS_WORKER_TIMEOUT');}
async function observations(s:Sources){
 const alerts=(await db.prepare('SELECT id,hold_id,phase FROM inventory_quality_alerts WHERE hold_id IN (?,?) ORDER BY id').bind(...s.holdIds).all<{id:string;hold_id:string;phase:string}>()).results;
 const recipients=(await db.prepare("SELECT n.record_id,n.user_id FROM staff_notifications n JOIN inventory_quality_alerts a ON a.id=n.record_id WHERE n.company_id=? AND n.record_type='quality_alert' AND a.hold_id IN (?,?) ORDER BY n.record_id,n.user_id").bind(s.f.company,...s.holdIds).all<{record_id:string;user_id:string}>()).results;
 const reservations=(await db.prepare("SELECT record_type,record_id,user_id FROM staff_notifications WHERE company_id=? AND message LIKE 'Stock reservation%' AND record_id IN (?,?,?) ORDER BY record_type,record_id,user_id").bind(s.f.company,s.salesId,s.fieldId,s.productionId).all<{record_type:string;record_id:string;user_id:string}>()).results;
 const mail=await db.prepare('SELECT status,attempts,provider_message_id,recipient_email,subject,body_text,adapter FROM commerce_customer_notifications WHERE id=?').bind(s.messageId).first<{status:string;attempts:number;provider_message_id:string|null;recipient_email:string;subject:string;body_text:string;adapter:string}>();
 return {alerts,recipients,reservations,mail};
}
async function sourceSnapshot(s:Sources){return JSON.stringify({sales:await db.prepare('SELECT version,state FROM commerce_orders WHERE id=?').bind(s.salesId).first(),field:await db.prepare('SELECT version,status FROM field_service_orders WHERE id=?').bind(s.fieldId).first(),manufacturing:await db.prepare('SELECT version,status FROM manufacturing_orders WHERE id=?').bind(s.productionId).first(),holds:(await db.prepare('SELECT id,version,status FROM inventory_quality_holds WHERE id IN (?,?) ORDER BY id').bind(...s.holdIds).all()).results,stock:(await db.prepare('SELECT product_id,quantity_milli,value_minor,version FROM commerce_stock WHERE warehouse_id=? ORDER BY product_id').bind(s.warehouse).all()).results});}
async function certificationSnapshot(){return JSON.stringify((await db.prepare(`SELECT 'business' kind,row_to_json(b)::text evidence FROM company_business_acceptance b UNION ALL SELECT 'release',row_to_json(a)::text FROM company_release_acceptances a UNION ALL SELECT 'review',row_to_json(r)::text FROM company_release_gate_reviews r ORDER BY kind,evidence`).all()).results);}
function assertEqual(actual:unknown,expected:unknown,code:string){if(JSON.stringify(actual)!==JSON.stringify(expected))throw new Error(code);}
function assertBlocked(value:Awaited<ReturnType<typeof observations>>,code:string){if(value.alerts.length||value.recipients.length||value.reservations.length||value.mail?.status!=='pending'||value.mail.attempts!==0||value.mail.provider_message_id!==null)throw new Error(code);}
const notice='Isolated engineering only: declared fictional native owner sources, actual local ClamAV, and no-network sandbox delivery. Company/provider certification remains pending.';
try{
 const seeded={} as Record<typeof names[number],Sources>;for(const name of names)seeded[name]=await seedBusinessWorkerSources(db,files,{now,label:`Compiled ${name} worker ${run}`,proof:'clamav',nativeOpening:true});sources=seeded;
 const enrollment={run,sourceFingerprint:artifact.sourceFingerprint,softwareVersion:artifact.softwareVersion,fileManifestSha256:artifact.fileManifestSha256};
 await enrollEngineeringVerification(db,{...enrollment,companyIds:[sources.enabled.f.company,sources.withdrawn.f.company,sources.expired.f.company],disabledCompanyIds:[sources.disabled.f.company]});
 await db.prepare('UPDATE engineering_verification_contexts SET enabled=false WHERE run_id=? AND company_id=?').bind(run,sources.withdrawn.f.company).run();
 await db.prepare('UPDATE engineering_verification_contexts SET expires_at=? WHERE run_id=? AND company_id=?').bind(Date.now()-1,run,sources.expired.f.company).run();
 if(await db.prepare('SELECT 1 FROM engineering_verification_contexts WHERE company_id=?').bind(sources.unenrolled.f.company).first())throw new Error('UNENROLLED_COMPANY_ALREADY_ENROLLED');
 // This selected reviewer is revoked before packaged jobs claim any alert. The independent owner remains eligible.
 await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(Date.now(),sources.enabled.qualityReviewer.assignmentId).run();
 const before=Object.fromEntries(await Promise.all(names.map(async name=>[name,await observations(sources[name])]))),nativeBefore=Object.fromEntries(await Promise.all(names.map(async name=>[name,await sourceSnapshot(sources[name])]))),certificationBefore=await certificationSnapshot();
 for(const name of names)assertBlocked(before[name],`NEW_NATIVE_SOURCE_ALREADY_CONSUMED_${name}`);
 const controlPath=join(directory,'business-production-worker-control-fixture.json');
 const controlBody=JSON.stringify({schemaVersion:1,scope:'isolated-engineering',run,sourceFingerprint:artifact.sourceFingerprint,softwareVersion:artifact.softwareVersion,releaseManifestSha256:artifact.fileManifestSha256,companies:Object.fromEntries(names.map(name=>[name,{companyId:sources[name].f.company,holds:sources[name].holdIds,salesId:sources[name].salesId,fieldId:sources[name].fieldId,productionId:sources[name].productionId,messageId:sources[name].messageId,fulfillmentId:sources[name].fulfillmentId,openingCounts:sources[name].openingCounts,evidence:sources[name].evidence}])),currentRecipients:{owner:sources.enabled.reviewer.userId,revoked:sources.enabled.qualityReviewer.userId,revokedAssignment:sources.enabled.qualityReviewer.assignmentId},notice},null,2)+'\n';
 await writeFile(controlPath,controlBody,{mode:0o600});const controlSha=createHash('sha256').update(controlBody).digest('hex');
 const preStartHeartbeat=await heartbeat();await startWorker();
 await until(async()=>{const value=await observations(sources.enabled);return value.alerts.length===2&&value.recipients.length===2&&value.reservations.length===5&&value.mail?.status==='sent'&&await heartbeat()>preStartHeartbeat;});
 const firstHeartbeat=await heartbeat();heartbeats.push(firstHeartbeat);await stopWorker();
 const first=await observations(sources.enabled);
 assertEqual(first.alerts.map(row=>row.phase).sort(),['due_soon','overdue'],'COMPILED_QUALITY_PHASES_MISMATCH');
 assertEqual(first.recipients.map(row=>row.user_id),[sources.enabled.reviewer.userId,sources.enabled.reviewer.userId],'COMPILED_CURRENT_QUALITY_RECIPIENT_MISMATCH');
 const nativeRecipients={commerce_order:[sources.enabled.f.owner.userId],field_service_order:[sources.enabled.f.owner.userId,sources.enabled.f.claimant.userId].sort(),manufacturing_order:[sources.enabled.reviewer.userId,sources.enabled.f.owner.userId].sort()};
 for(const [type,expected] of Object.entries(nativeRecipients))assertEqual(first.reservations.filter(row=>row.record_type===type).map(row=>row.user_id).sort(),expected,`COMPILED_RESERVATION_RECIPIENT_MISMATCH_${type}`);
 if(first.mail?.status!=='sent'||first.mail.attempts!==1||first.mail.provider_message_id!==`sandbox-${sources.enabled.messageId}`)throw new Error('COMPILED_SANDBOX_DELIVERY_MISMATCH');
 for(const name of names.filter(name=>name!=='enabled'))assertBlocked(await observations(sources[name]),`COMPILED_BLOCKED_COMPANY_SIDE_EFFECT_${name}`);
 const replacement=await sources.enabled.f.grant(sources.enabled.qualityReviewer,'quality_manager');
 await db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(sources.enabled.qualityReviewer.modules),replacement).run();
 await startWorker();await until(async()=>{const value=await observations(sources.enabled);return value.recipients.length===4&&await heartbeat()>firstHeartbeat;});
 const secondHeartbeat=await heartbeat();heartbeats.push(secondHeartbeat);await stopWorker();
 const second=await observations(sources.enabled);
 assertEqual(second.alerts,first.alerts,'QUALITY_ALERT_DUPLICATED_AFTER_RESTART');assertEqual(second.reservations,first.reservations,'RESERVATION_NOTICE_DUPLICATED_AFTER_RESTART');assertEqual(second.mail,first.mail,'DELIVERY_SNAPSHOT_OR_ATTEMPT_CHANGED_AFTER_RESTART');
 for(const alert of second.alerts)assertEqual(second.recipients.filter(row=>row.record_id===alert.id).map(row=>row.user_id).sort(),[sources.enabled.reviewer.userId,sources.enabled.qualityReviewer.userId].sort(),'QUALITY_RECIPIENT_RECOVERY_MISMATCH');
 const nativeSourceSnapshots={} as Record<typeof names[number],{beforeSha256:string;afterSha256:string}>;
 for(const name of names){if(name!=='enabled')assertBlocked(await observations(sources[name]),`COMPILED_RESTART_BLOCKED_COMPANY_SIDE_EFFECT_${name}`);const retained=await sourceSnapshot(sources[name]);assertEqual(retained,nativeBefore[name],`COMPILED_NATIVE_SOURCE_OR_STOCK_CHANGED_${name}`);nativeSourceSnapshots[name]={beforeSha256:createHash('sha256').update(nativeBefore[name]).digest('hex'),afterSha256:createHash('sha256').update(retained).digest('hex')};const after=await observations(sources[name]),old=before[name].mail;assertEqual(after.mail&&{recipient_email:after.mail.recipient_email,subject:after.mail.subject,body_text:after.mail.body_text,adapter:after.mail.adapter},old&&{recipient_email:old.recipient_email,subject:old.subject,body_text:old.body_text,adapter:old.adapter},`COMPILED_MESSAGE_SOURCE_CHANGED_${name}`);}
 if(await certificationSnapshot()!==certificationBefore)throw new Error('ENGINEERING_WORKER_CHANGED_CERTIFICATION');
 const passed=['compiled_quality_due_soon_overdue','compiled_sales_field_manufacturing_reservations','compiled_sandbox_delivery_once','disabled_company','unenrolled_company','withdrawn_company','expired_company','current_recipient_revocation_recovery','restart_deduplication','native_sources_and_stock_unchanged','certification_unchanged'];
 const result={schemaVersion:1,scope:'isolated-engineering',run,sourceFingerprint:artifact.sourceFingerprint,softwareVersion:artifact.softwareVersion,releaseManifestSha256:artifact.fileManifestSha256,compiledJobsSha256:checksums['tools/jobs.mjs'],passed,nativeSourceSnapshots,controlFixture:{path:controlPath,sha256:controlSha},companies:Object.fromEntries(names.map(name=>[name,sources[name].f.company])),observations:{first,afterRestart:second,blocked:Object.fromEntries(await Promise.all(names.filter(name=>name!=='enabled').map(async name=>[name,await observations(sources[name])])) )},currentRecipients:{revokedAssignment:sources.enabled.qualityReviewer.assignmentId,replacementAssignment:replacement,reviewerId:sources.enabled.reviewer.userId,recoveredReviewerId:sources.enabled.qualityReviewer.userId},restart:{cohortJobsPaused:true,cohortJobsPid:pause.jobsPid,workerPids,firstWorkerPid:workerPids[0],restartedWorkerPid:workerPids[1],preStartHeartbeat,firstHeartbeat,secondHeartbeat,heartbeats},environment:{port:3104,databases:8,node:'24.20.0',bun:'1.2.17',provider:'sandbox',scanner:'actual local ClamAV'},notice,completedAt:new Date().toISOString()};
 await writeFile(join(directory,'business-production-worker-evidence.json'),JSON.stringify(result,null,2)+'\n',{mode:0o600});
 console.log(JSON.stringify({run,passed,evidencePath:join(directory,'business-production-worker-evidence.json'),controlFixtureSha256:controlSha,notice}));
}finally{await stopWorker();await closeRuntime();await Promise.all(Object.values(databases).map(db=>db.close()));}
