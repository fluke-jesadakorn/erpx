/** Lives in the owned network-none PostgreSQL namespace for one compiled run. */
import {access,readFile,writeFile} from 'node:fs/promises';
import {createNativeEngineeringCohort} from './native-cohort';
import {prepareNativeServiceChild} from './native-service-child';
const run=process.env.ERPX_ENGINEERING_RUN,caseRoot=process.env.ERPX_ENGINEERING_CASE_ROOT;
if(!run||!/^[a-f0-9]{12}$/.test(run)||caseRoot!=='/case')throw Error('ENGINEERING_RUN_REQUIRED');
const directory=`${caseRoot}/.runtime/browser/${run}`,release=JSON.parse(await readFile(`${directory}/release/release.json`,'utf8'));
const cohort=await createNativeEngineeringCohort({directory,run,sourceFingerprint:release.sourceFingerprint,fileManifestSha256:release.fileManifestSha256,adminUrl:'postgresql://erpx_test_admin:synthetic@127.0.0.1:5432/postgres'});
try{
 const child=await prepareNativeServiceChild(directory,run);
 await writeFile(`${directory}/engineering-attestation.json`,JSON.stringify({scope:'isolated-engineering',run,sourceFingerprint:release.sourceFingerprint,softwareVersion:release.softwareVersion,fileManifestSha256:release.fileManifestSha256,nativeLeaseSha256:cohort.rosterSha256,nativeServiceChildSha256:child.sha256}),{mode:0o600,flag:'wx'});
 await writeFile(`${directory}/cohort-ready.json`,JSON.stringify({run,rosterSha256:cohort.rosterSha256,sourceFingerprint:release.sourceFingerprint,fileManifestSha256:release.fileManifestSha256}),{mode:0o600,flag:'wx'});
 for(;;){try{await access(`${directory}/cohort-stop`);break;}catch{await Bun.sleep(250);}}
}finally{await cohort.cleanup();}
await writeFile(`${directory}/cohort-cleaned`, 'ok\n',{mode:0o600,flag:'wx'});
