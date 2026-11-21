/** Actual Service-entry process exercising current private attestation reads. */
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {chmod,readFile,rm,symlink,writeFile} from 'node:fs/promises';
import {engineeringVerificationContext,isEngineeringVerificationOrigin} from '@erpx/server-platform_administration/public/engineering-verification';
const directory=process.env.ERPX_DATA_DIR!;
const path=`${directory}/native-service-child.json`,attestationPath=`${directory}/engineering-attestation.json`,entry=process.argv[1]!,release=`${directory}/release`;
const original=await readFile(path),attestation=await readFile(attestationPath),entryBytes=await readFile(entry),releasePath=`${release}/release.json`,releaseBytes=await readFile(releasePath);
const baseline=engineeringVerificationContext();assert(baseline?.nativeRoster);assert(isEngineeringVerificationOrigin(new URL('http://127.0.0.1:3104'),baseline));assert(!isEngineeringVerificationOrigin(new URL('http://127.0.0.1:3213'),baseline));
for(const changed of [{version:2},{appId:'account'},{port:3214},{gatewayPort:3105},{run:'111111111111'},{nonce:'0'.repeat(32)},{sourceFingerprint:'0'.repeat(64)},{fileManifestSha256:'0'.repeat(64)}]){
 const edited=Buffer.from(JSON.stringify({...JSON.parse(original.toString()),...changed}));await writeFile(path,edited);await writeFile(attestationPath,JSON.stringify({...JSON.parse(attestation.toString()),nativeServiceChildSha256:createHash('sha256').update(edited).digest('hex')}));assert.equal(engineeringVerificationContext(),null);
}
await writeFile(path,original);await writeFile(attestationPath,attestation);
await chmod(path,0o644);assert.equal(engineeringVerificationContext(),null);await chmod(path,0o600);
await rm(path);assert.equal(engineeringVerificationContext(),null);await writeFile(path,'{',{mode:0o600});await writeFile(attestationPath,JSON.stringify({...JSON.parse(attestation.toString()),nativeServiceChildSha256:createHash('sha256').update('{').digest('hex')}));assert.equal(engineeringVerificationContext(),null);await writeFile(path,original);await writeFile(attestationPath,attestation);
const rosterPath=`${directory}/native-engineering-leases.json`,roster=await readFile(rosterPath);await writeFile(rosterPath,'{}');assert.equal(engineeringVerificationContext(),null);await writeFile(rosterPath,roster);
await writeFile(entry,Buffer.concat([entryBytes,Buffer.from('\n// changed\n')]));assert.equal(engineeringVerificationContext(),null);await writeFile(entry,entryBytes);
await writeFile(releasePath,JSON.stringify({...JSON.parse(releaseBytes.toString()),scope:'different'}));assert.equal(engineeringVerificationContext(),null);await writeFile(releasePath,releaseBytes);
for(const key of ['ERPX_RESOURCE_ROOT','ERPX_REPOSITORY_ROOT'] as const){const prior=process.env[key];process.env[key]=directory;assert.equal(engineeringVerificationContext(),null);process.env[key]=prior;}
const alias=`${directory}/release-alias`;await symlink(release,alias);const priorRoot=process.env.ERPX_RESOURCE_ROOT;process.env.ERPX_RESOURCE_ROOT=alias;assert.equal(engineeringVerificationContext(),null);process.env.ERPX_RESOURCE_ROOT=priorRoot;await rm(alias);
const caseAlias=`${directory}-alias`;await symlink(directory,caseAlias);process.env.ERPX_DATA_DIR=caseAlias;assert.equal(engineeringVerificationContext(),null);process.env.ERPX_DATA_DIR=directory;await rm(caseAlias);
const priorNonce=process.env.ERPX_ENGINEERING_SERVICE_NONCE;delete process.env.ERPX_ENGINEERING_SERVICE_NONCE;assert.equal(engineeringVerificationContext(),null);process.env.ERPX_ENGINEERING_SERVICE_NONCE=priorNonce;
const prior=process.argv[1];process.argv[1]=`${directory}/wrong-entry.mjs`;assert.equal(engineeringVerificationContext(),null);process.argv[1]=prior;
assert(engineeringVerificationContext()?.nativeRoster);console.log('NATIVE_SERVICE_CHILD_PROOF_PASS');
