/** Lives only inside the companion network-none PostgreSQL qualification. */
import {createHash} from 'node:crypto';
import {access,mkdir,writeFile} from 'node:fs/promises';
import {createNativeEngineeringCohort} from '../../../scripts/testing/engineering/native-cohort';
const run=process.env.ERPX_FENCE_RUN;if(!run||!/^[a-f0-9]{12}$/.test(run))throw Error('FENCE_RUN_REQUIRED');
const dir=`/tmp/.runtime/browser/${run}`,source='a'.repeat(64),bytes=JSON.stringify({'server.js':'b'.repeat(64)}),digest=createHash('sha256').update(bytes).digest('hex');
await mkdir(dir+'/release',{recursive:true,mode:0o700});await writeFile(dir+'/release/release-files.json',bytes);await writeFile(dir+'/release/release.json',JSON.stringify({sourceFingerprint:source,softwareVersion:`release-${source}`,fileManifestSha256:digest}));
const cohort=await createNativeEngineeringCohort({directory:dir,run,sourceFingerprint:source,fileManifestSha256:digest,adminUrl:'postgresql://erpx_test_admin:synthetic@127.0.0.1:5432/postgres'});
await writeFile(dir+'/engineering-attestation.json',JSON.stringify({scope:'isolated-engineering',run,sourceFingerprint:source,softwareVersion:`release-${source}`,fileManifestSha256:digest,nativeLeaseSha256:cohort.rosterSha256}),{mode:0o600,flag:'wx'});
console.log('EIGHT_READY');try{for(;;){try{await access('/tmp/release');break;}catch{await Bun.sleep(250);}}}finally{await cohort.cleanup();}console.log('EIGHT_CLEANED');
