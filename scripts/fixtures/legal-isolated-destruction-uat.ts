/** Run an already independently authorized manifest in one exact disposable run. */
import {readFile,writeFile,realpath} from 'node:fs/promises';
import {resolve,join} from 'node:path';
import {createHash} from 'node:crypto';
import {PostgresDatabase} from '../../src/server/runtime/postgres';
import {LocalFileStore} from '../../src/server/runtime/filesystem';
import {drainLegalDestruction} from '../../src/server/legal/retention';
import type {SqlDatabase} from '../../src/server/runtime/types';
import type {LegalDestructionManifest} from '../../src/server/legal/destruction-sources';

const root=resolve(import.meta.dir,'../..'),[run,phase,requestId]=process.argv.slice(2);
if(!run||!/^[a-f0-9]{12}$/.test(run)||!['fault','resume','hold'].includes(phase)||!requestId||!/^[a-f0-9-]{36}$/.test(requestId))throw new Error('EXACT_ISOLATED_DESTRUCTION_REQUEST_REQUIRED');
const directory=join(root,'.runtime/browser',run),state=JSON.parse(await readFile(join(directory,'state.json'),'utf8'));
if(state.run!==run||state.port!==3104||state.origin!=='http://127.0.0.1:3104'||state.prefix!==`erpx_browser_${run}`||!state.fixture?.legal||process.env.ERPX_SERVER_SCRIPT_SOURCE_FINGERPRINT!==state.sourceFingerprint)throw new Error('FROZEN_LEGAL_UAT_SOURCE_REQUIRED');
const release=await realpath(join(directory,'release'));if(release!==join(directory,'release'))throw new Error('DIRECT_ISOLATED_RELEASE_REQUIRED');
const artifact=JSON.parse(await readFile(join(release,'release.json'),'utf8')),attestation=JSON.parse(await readFile(join(directory,'engineering-attestation.json'),'utf8'));
if(attestation.scope!=='isolated-engineering'||attestation.run!==run||attestation.sourceFingerprint!==state.sourceFingerprint||attestation.softwareVersion!==artifact.softwareVersion||artifact.sourceFingerprint!==state.sourceFingerprint||artifact.softwareVersion!==`release-${state.sourceFingerprint}`||artifact.fileManifestSha256!==attestation.fileManifestSha256||createHash('sha256').update(await readFile(join(release,'release-files.json'))).digest('hex')!==artifact.fileManifestSha256)throw new Error('EXACT_ATTESTED_DESTRUCTION_BUILD_REQUIRED');
const databases=JSON.parse(await readFile(join(directory,'databases.json'),'utf8')).databases,url=new URL(databases.H001_DB);
if(url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||url.pathname!==`/erpx_browser_${run}_h001_db`)throw new Error('EXACT_DESTRUCTION_DATABASE_REQUIRED');
const db=new PostgresDatabase({connectionString:url.href,max:2}),files=new LocalFileStore(join(directory,'files/h001'),db);
try{
 if(phase==='hold'){
  const document=await db.prepare("SELECT item.id,item.company_id,doc.object_key,doc.checksum,(SELECT count(*)::int FROM legal_hold_targets target JOIN legal_holds hold ON hold.id=target.hold_id WHERE target.item_id=item.id AND hold.state<>'released') hold_count FROM legal_items item JOIN legal_documents doc ON doc.item_id=item.id WHERE item.id=? AND doc.object_key IS NOT NULL ORDER BY doc.created_at LIMIT 1").bind(requestId).first<{id:string;company_id:string;object_key:string;checksum:string;hold_count:number}>();
  if(!document||document.company_id!==state.fixture.legal.companies.A.id||document.hold_count<1)throw new Error('EXACT_NORMAL_LOGIN_HELD_ORIGINAL_REQUIRED');
  if(!await db.prepare("SELECT 1 FROM engineering_verification_contexts marker JOIN legal_settings legal ON legal.company_id=marker.company_id WHERE current_database()=? AND marker.company_id=? AND marker.run_id=? AND marker.source_fingerprint=? AND marker.software_version=? AND marker.release_manifest_sha256=? AND marker.enabled AND marker.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND legal.enabled").bind(`erpx_browser_${run}_h001_db`,document.company_id,run,state.sourceFingerprint,artifact.softwareVersion,artifact.fileManifestSha256).first())throw new Error('PRESERVATION_ENGINEERING_ENROLLMENT_WITHDRAWN');
  const before=await files.get(document.object_key);if(!before||createHash('sha256').update(Buffer.from(await before.arrayBuffer())).digest('hex')!==document.checksum)throw new Error('HELD_ORIGINAL_CHECKSUM_MISMATCH');
  const attempts=[async()=>files.delete(document.object_key),async()=>files.put(document.object_key,'forbidden held-byte replacement')],codes:string[]=[];
  for(const attempt of attempts){try{await attempt();throw new Error('HELD_BYTE_MUTATION_WAS_ALLOWED');}catch(error){if(!(error instanceof Error)||error.message!=='LEGAL_HOLD_BLOCKS_FILE_MUTATION')throw error;codes.push(error.message);}}
  const after=await files.get(document.object_key);if(!after||createHash('sha256').update(Buffer.from(await after.arrayBuffer())).digest('hex')!==document.checksum)throw new Error('HELD_ORIGINAL_CHANGED');
  const result={scope:'isolated-engineering',run,sourceFingerprint:state.sourceFingerprint,softwareVersion:artifact.softwareVersion,itemId:document.id,companyId:document.company_id,remainingHolds:document.hold_count,originalSha256:document.checksum,deleteRefused:true,replacementRefused:true,codes,notice:'Normal-login cyclic Legal holds; native storage mutation attempts refused; original checksum retained'};
  const output=join(directory,`legal-preservation-${requestId}-hold.json`);await writeFile(output,JSON.stringify(result,null,2)+'\n',{mode:0o600});console.log(JSON.stringify({phase,evidencePath:output}));
 }else{
 const request=await db.prepare('SELECT destruction.*,item.company_id FROM legal_destructions destruction JOIN legal_items item ON item.id=destruction.item_id WHERE destruction.id=?').bind(requestId).first<{id:string;item_id:string;company_id:string;state:string;approved_by:string|null;requested_by:string;manifest:string}>();
 if(!request||!request.approved_by||request.approved_by===request.requested_by||request.company_id!==state.fixture.legal.companies.A.id||request.state!==(phase==='fault'?'approved':'executing'))throw new Error('NORMAL_LOGIN_INDEPENDENT_MANIFEST_APPROVAL_REQUIRED');
 const authorized=async(tx:SqlDatabase)=>{
  if(!await tx.prepare("SELECT 1 FROM engineering_verification_contexts marker JOIN legal_settings legal ON legal.company_id=marker.company_id WHERE current_database()=? AND marker.company_id=? AND marker.run_id=? AND marker.source_fingerprint=? AND marker.software_version=? AND marker.release_manifest_sha256=? AND marker.enabled AND marker.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND legal.enabled FOR SHARE OF marker,legal").bind(`erpx_browser_${run}_h001_db`,request.company_id,run,state.sourceFingerprint,artifact.softwareVersion,artifact.fileManifestSha256).first())throw new Error('DESTRUCTION_ENGINEERING_ENROLLMENT_WITHDRAWN');
 };
 await authorized(db);
 const next=await db.prepare("SELECT id FROM legal_destructions WHERE state IN ('approved','executing') AND attempts<6 ORDER BY created_at LIMIT 1").first<string>('id');if(next!==requestId)throw new Error('EXACT_DESTRUCTION_QUEUE_HEAD_REQUIRED');
 const manifest=JSON.parse(request.manifest) as LegalDestructionManifest,manifestSha256=createHash('sha256').update(request.manifest).digest('hex');
 if(!manifest.keys.length||!manifest.canonicalSources?.length)throw new Error('CANONICAL_DESTRUCTION_MANIFEST_REQUIRED');
 const beforeFiles=await Promise.all(manifest.keys.map(async key=>({key,present:Boolean(await files.head(key))})));
 const destroy=files.destroyPreserved.bind(files);
 files.destroyPreserved=async(keys,authorization)=>{
  await authorized(authorization.database);
  if(phase==='fault'){await destroy(keys.slice(0,1),authorization);throw new Error('INJECTED_INTERRUPTION_AFTER_FIRST_ORIGINAL');}
  await destroy(keys,authorization);
 };
 await drainLegalDestruction(db,files);
 const after=await db.prepare('SELECT state,error,manifest,completed_at FROM legal_destructions WHERE id=?').bind(requestId).first<{state:string;error:string|null;manifest:string;completed_at:number|null}>();
 if(!after||createHash('sha256').update(after.manifest).digest('hex')!==manifestSha256||after.state!==(phase==='fault'?'executing':'completed')||(phase==='fault'?after.error!=='DESTRUCTION_REQUIRES_REVIEW':after.error!==null))throw new Error('DESTRUCTION_JOURNAL_OR_MANIFEST_MISMATCH');
 const remaining=(await Promise.all(manifest.keys.map(async key=>Boolean(await files.head(key))))).filter(Boolean).length;
 if(phase==='fault'&&remaining>=beforeFiles.filter(file=>file.present).length||phase==='resume'&&remaining!==0)throw new Error('DESTRUCTION_BYTE_RECOVERY_MISMATCH');
 const sourceState=(await db.prepare("SELECT source_state FROM attachments WHERE id=?").bind(manifest.canonicalSources[0].id).first<string>('source_state'));
 const ledgerStored=Number(await db.prepare("SELECT count(*) n FROM attachment_storage_ledger WHERE attachment_id=? AND state='stored'").bind(manifest.canonicalSources[0].id).first('n'));
 const certificateCount=Number(await db.prepare("SELECT count(*) n FROM legal_events WHERE item_id=? AND action='retention.destroyed'").bind(request.item_id).first('n'));
 if(phase==='resume'&&(sourceState!=='destroyed'||ledgerStored!==0||certificateCount!==1))throw new Error('DESTRUCTION_SOURCE_QUOTA_OR_CERTIFICATE_MISMATCH');
 const result={sourceState,ledgerStored,certificateCount,scope:'isolated-engineering',run,sourceFingerprint:state.sourceFingerprint,softwareVersion:artifact.softwareVersion,requestId,itemId:request.item_id,companyId:request.company_id,phase,state:after.state,error:after.error,manifestSha256,originalChecksums:manifest.documents.map(document=>document.checksum),beforeFiles,remaining,completedAt:after.completed_at,notice:'Normal-login owner and independent approval; injected isolated fault; certification unchanged; automatic production destruction remains deferred'};
 const output=join(directory,`legal-destruction-${requestId}-${phase}.json`);await writeFile(output,JSON.stringify(result,null,2)+'\n',{mode:0o600});console.log(JSON.stringify({phase,state:after.state,evidencePath:output}));
 }
}finally{await db.close();}
