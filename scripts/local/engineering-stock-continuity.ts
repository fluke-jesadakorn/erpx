/** Strict disposable-cohort stock proof; never connects to development or production. */
import {createHash} from 'node:crypto';
import {chmod,readFile,realpath,writeFile} from 'node:fs/promises';
import {join,resolve,sep} from 'node:path';
import {PostgresDatabase} from '../../src/server/runtime/postgres';
import {readStockSnapshot,reconcileStockSnapshots,type StockSnapshot} from '../fixtures/engineering-stock-continuity';
import {runtimeSourceFingerprint,verificationSourceFingerprint} from './source-fingerprint';

const root=resolve(import.meta.dir,'../..'),run=process.argv[2],project=process.argv[3],mode=process.argv[4],requested=process.argv[5];
if(!run||!/^[a-f0-9]{12}$/.test(run)||!['desktop','mobile'].includes(project)||!['baseline','final'].includes(mode)||!requested)throw new Error('EXACT_STOCK_COHORT_PROJECT_PHASE_REQUIRED');
const directory=await realpath(resolve(root,requested));if(!directory.startsWith(join(root,'.scratch/dev/engineering-completion/runs')+sep))throw new Error('PRIVATE_ENGINEERING_RUN_REQUIRED');
const cohort=join(root,'.runtime/browser',run),state=JSON.parse(await readFile(join(cohort,'state.json'),'utf8')),attestation=JSON.parse(await readFile(join(cohort,'engineering-attestation.json'),'utf8'));
const release=await realpath(join(cohort,'release')),artifact=JSON.parse(await readFile(join(release,'release.json'),'utf8')),sourceFingerprint=await runtimeSourceFingerprint(root),verificationFingerprint=await verificationSourceFingerprint(root,sourceFingerprint);
if(state.run!==run||state.prefix!==`erpx_browser_${run}`||state.port!==3104||state.ready!==true||state.diagnostic===true||state.sourceFingerprint!==sourceFingerprint||artifact.sourceFingerprint!==sourceFingerprint||state.softwareVersion!==artifact.softwareVersion||state.release?.fileManifestSha256!==artifact.fileManifestSha256||attestation.scope!=='isolated-engineering'||attestation.run!==run||attestation.sourceFingerprint!==sourceFingerprint||attestation.fileManifestSha256!==artifact.fileManifestSha256)throw new Error('STOCK_COMPILED_COHORT_ATTESTATION_MISMATCH');
const manifest=JSON.parse(await readFile(join(cohort,'databases.json'),'utf8')),bindings=['CONTROL_DB','CONTROL_READ_DB','H001_DB','H001_READ_DB','H002_DB','H002_READ_DB','H003_DB','H003_READ_DB'];
if(Object.keys(manifest.databases).length!==8||!bindings.every(binding=>manifest.databases[binding]))throw new Error('EXACT_EIGHT_STOCK_DATABASES_REQUIRED');
for(const binding of bindings){const url=new URL(manifest.databases[binding]);if(!['postgres:','postgresql:'].includes(url.protocol)||url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||url.pathname!==`/erpx_browser_${run}_${binding.toLowerCase()}`)throw new Error('EXACT_DISPOSABLE_STOCK_DATABASE_REQUIRED');}
const snapshots:Array<{holdingId:string;snapshot:StockSnapshot}>=[];
for(const holdingId of ['h001','h002','h003']){
 const db=new PostgresDatabase({connectionString:manifest.databases[`${holdingId.toUpperCase()}_DB`],max:1});
 try{snapshots.push({holdingId,snapshot:await db.transaction(async tx=>{await tx.prepare('SET TRANSACTION READ ONLY').run();return readStockSnapshot(tx);})});}finally{await db.close();}
}
const identity={schemaVersion:1,scope:'isolated-engineering',run,project,sourceFingerprint,verificationFingerprint,softwareVersion:artifact.softwareVersion,fileManifestSha256:artifact.fileManifestSha256};
const baselineName=`stock-${project}-baseline.json`,finalName=`stock-${project}.json`;
async function save(name:string,value:unknown){await writeFile(join(directory,name),JSON.stringify(value,null,2)+'\n',{mode:0o600});await chmod(join(directory,name),0o600);}
if(mode==='baseline')await save(baselineName,{...identity,snapshots,createdAt:new Date().toISOString()});
else{
 const bytes=await readFile(join(directory,baselineName)),baseline=JSON.parse(bytes.toString());
 if(Object.entries(identity).some(([key,value])=>baseline[key]!==value)||baseline.snapshots?.length!==3)throw new Error('EXACT_STOCK_BASELINE_REQUIRED');
 const results=snapshots.map(({holdingId,snapshot})=>{const previous=baseline.snapshots.find((item:{holdingId:string})=>item.holdingId===holdingId);if(!previous)throw new Error('STOCK_HOLDING_BASELINE_MISSING');return {holdingId,...reconcileStockSnapshots(previous.snapshot,snapshot)};});
 const passed=results.every(item=>item.passed)&&results.some(item=>item.stockAssertions.length>0)&&results.some(item=>item.newMovements>0);
 const snapshotName=`stock-${project}-final-snapshot.json`;await save(snapshotName,{...identity,snapshots});
 await save(finalName,{...identity,passed,baselineArtifact:baselineName,baselineArtifactSha256:createHash('sha256').update(bytes).digest('hex'),finalSnapshotArtifact:snapshotName,finalSnapshotArtifactSha256:createHash('sha256').update(await readFile(join(directory,snapshotName))).digest('hex'),results,completedAt:new Date().toISOString()});
 if(!passed)throw new Error('STOCK_WIP_GL_OR_SOURCE_MOVEMENT_DIFFERENCE');
}
if(await runtimeSourceFingerprint(root)!==sourceFingerprint||await verificationSourceFingerprint(root,sourceFingerprint)!==verificationFingerprint)throw new Error('STOCK_SOURCE_CHANGED_DURING_VERIFICATION');
console.log(JSON.stringify({run,project,phase:mode,artifact:mode==='baseline'?baselineName:finalName,holdings:3}));
