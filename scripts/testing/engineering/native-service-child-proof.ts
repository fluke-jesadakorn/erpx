/** Small filesystem-only child-process proof; it does not access a database. */
import {execFile} from 'node:child_process';
import {createHash} from 'node:crypto';
import {mkdir,mkdtemp,readFile,realpath,rm,writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {promisify} from 'node:util';
import {prepareNativeServiceChild} from './native-service-child';
import {nativeEngineeringBindings} from '@erpx/server-runtime/public/native-engineering-roster';
const exec=promisify(execFile),root=resolve('.'),base=await realpath(await mkdtemp(join(tmpdir(),'erpx-service-child-'))),run='abcdef123456',directory=join(base,'.runtime/browser',run),release=join(directory,'release'),source='a'.repeat(64),version=`release-${source}`;
try{
 await mkdir(join(release,'apps/service'),{recursive:true,mode:0o700});await mkdir(join(base,'node_modules/@erpx/catalogue'),{recursive:true});
 const probe=join(base,'probe.js');await exec(process.execPath,['build',join(root,'tests/support/runtime/native-service-child-probe.ts'),'--target=bun','--conditions=react-server','--external','@erpx/catalogue/release-artifact.json','--define','process.env.NODE_ENV="production"','--outfile',probe],{cwd:root});
 await writeFile(join(base,'node_modules/@erpx/catalogue/package.json'),JSON.stringify({name:'@erpx/catalogue',type:'module',exports:{'./release-artifact.json':'./release-artifact.json'}}));await writeFile(join(base,'node_modules/@erpx/catalogue/release-artifact.json'),JSON.stringify({softwareVersion:version}));
 const entry=join(release,'apps/service/entry.mjs'),entryBytes=`await import(${JSON.stringify(probe)});\n`;await writeFile(entry,entryBytes);
 const manifest=JSON.stringify({'apps/service/entry.mjs':createHash('sha256').update(entryBytes).digest('hex')}),manifestSha=createHash('sha256').update(manifest).digest('hex');await writeFile(join(release,'release-files.json'),manifest);await writeFile(join(release,'release.json'),JSON.stringify({scope:'isolated-engineering-service-only',sourceFingerprint:source,softwareVersion:version,fileManifestSha256:manifestSha}));
 const roster={version:1,run,sourceFingerprint:source,fileManifestSha256:manifestSha,postmasterStartedMs:123456,entries:nativeEngineeringBindings.map((binding,i)=>({binding,token:(i+1).toString(16).padStart(32,'0'),oid:100+i}))},rosterBytes=Buffer.from(JSON.stringify(roster));await writeFile(join(directory,'native-engineering-leases.json'),rosterBytes,{mode:0o600});
 const child=await prepareNativeServiceChild(directory,run),nonce=(await readFile(join(directory,'native-service-child-nonce'),'utf8')).trim();await writeFile(join(directory,'engineering-attestation.json'),JSON.stringify({scope:'isolated-engineering',run,sourceFingerprint:source,softwareVersion:version,fileManifestSha256:manifestSha,nativeLeaseSha256:createHash('sha256').update(rosterBytes).digest('hex'),nativeServiceChildSha256:child.sha256}),{mode:0o600});
 const env={...process.env,NODE_ENV:'production' as const,PORT:'3213',HOSTNAME:'127.0.0.1',ERPX_ENVIRONMENT:'local',ERPX_PRODUCT_ENVIRONMENT:'development',ERPX_APP_ID:'service',ERPX_APP_PORT:'3213',ERPX_BROWSER_SIMULATION_PORT:'3104',ERPX_ENGINEERING_VERIFICATION_RUN:run,ERPX_ENGINEERING_SERVICE_NONCE:nonce,ERPX_DATA_DIR:directory,ERPX_RESOURCE_ROOT:`${release}/`,ERPX_REPOSITORY_ROOT:`${release}/`};
 const result=await exec(process.execPath,[entry],{env,cwd:release});if(result.stdout.trim()!=='NATIVE_SERVICE_CHILD_PROOF_PASS')throw Error('NATIVE_SERVICE_CHILD_PROOF_FAILED');console.log('native Service child filesystem proof passed');
}finally{await rm(base,{recursive:true,force:true});}
