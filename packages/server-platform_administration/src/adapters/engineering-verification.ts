import "server-only";
import { memoizeSyncInRequest } from "@erpx/server-runtime/public/runtime/request-memo";
import { engineeringCompanySource } from "@erpx/server-organization/public/engineering-company-source";
import { createHash } from "node:crypto";
import { closeSync, constants, fstatSync, lstatSync, openSync, readFileSync, readSync, realpathSync } from "node:fs";
import { join, resolve } from "node:path";
import artifact from "@erpx/catalogue/release-artifact.json";
import {validateNativeEngineeringRoster,type NativeEngineeringRoster} from "@erpx/server-runtime/public/native-engineering-roster";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { createEngineeringVerificationApplication } from "@erpx/server-platform_administration/application/engineering-verification";
import { isEngineeringVerificationOrigin as acceptsOrigin, type EngineeringAttestation, type EngineeringAttestationFacts } from "@erpx/server-platform_administration/domain/engineering-verification";

const manifestDigestKey=Symbol.for("erpx.engineering.manifestDigest");
function manifestDigest(path:string):string {
  // Read the current bytes on every guard. Only identical bytes in this one
  // HTTP invocation reuse a digest; tampering, symlinks and authority changes
  // still flow through the original checks, including inside transactions.
  const bytes=readFileSync(path);
  const entries=memoizeSyncInRequest(manifestDigestKey,()=>new Map<string,{bytes:Buffer;digest:string}>());
  const prior=entries.get(path);
  if(prior?.bytes.equals(bytes))return prior.digest;
  const digest=createHash("sha256").update(bytes).digest("hex");
  entries.set(path,{bytes,digest}); return digest;
}

function readPrivateNativeRoster(path:string):Buffer|null {
  let fd:number|undefined;
  try {
    fd=openSync(path,constants.O_RDONLY|constants.O_NOFOLLOW);
    const info=fstatSync(fd),current=lstatSync(path);
    if(!info.isFile()||current.isSymbolicLink()||info.dev!==current.dev||info.ino!==current.ino||(info.mode&0o777)!==0o600||info.uid!==process.getuid?.()||info.size>16384||realpathSync(path)!==path)return null;
    const bytes=Buffer.alloc(info.size+1),length=readSync(fd,bytes,0,bytes.length,0),after=fstatSync(fd);
    if(length!==info.size||after.size!==info.size||after.mtimeMs!==info.mtimeMs||after.ctimeMs!==info.ctimeMs)return null;
    return bytes.subarray(0,length);
  }catch{return null;}finally{if(fd!==undefined)closeSync(fd);}
}

function readAttestation(input: string, run: string, child?:Record<string,string|undefined>): EngineeringAttestationFacts | null {
  try {
    const directory = realpathSync(resolve(input)); if (!directory.endsWith(`/.runtime/browser/${run}`)||(child&&resolve(input)!==directory)) return null;
    const release = realpathSync(join(directory, "release")); if (release !== join(directory, "release")) return null;
    const attestationPath = realpathSync(join(directory, "engineering-attestation.json")); if (attestationPath !== join(directory, "engineering-attestation.json")) return null;
    const attestation = JSON.parse(readFileSync(attestationPath, "utf8")), releaseInfo = JSON.parse(readFileSync(join(release, "release.json"), "utf8"));
    // Never accept an inline roster from an attestation. The private roster is
    // source/release-bound by its hash and is reread for every authorization.
    delete attestation.nativeRoster;
    if(attestation.nativeLeaseSha256!==undefined){
      if(!/^[a-f0-9]{64}$/.test(attestation.nativeLeaseSha256))return null;
      const directoryInfo=lstatSync(directory),attestationInfo=lstatSync(attestationPath);
      if(!directoryInfo.isDirectory()||directoryInfo.isSymbolicLink()||(directoryInfo.mode&0o077)!==0||directoryInfo.uid!==process.getuid?.()||!attestationInfo.isFile()||attestationInfo.isSymbolicLink()||(attestationInfo.mode&0o777)!==0o600||attestationInfo.uid!==process.getuid?.())return null;
      const rosterPath=join(directory,'native-engineering-leases.json'),bytes=readPrivateNativeRoster(rosterPath);
      if(!bytes||createHash('sha256').update(bytes).digest('hex')!==attestation.nativeLeaseSha256)return null;
      const roster:unknown=JSON.parse(bytes.toString('utf8'));if(!validateNativeEngineeringRoster(roster,run,attestation.sourceFingerprint,attestation.fileManifestSha256))return null;
      attestation.nativeRoster=roster;
    }
    const manifestPath = realpathSync(join(release, "release-files.json")); if (manifestPath !== join(release, "release-files.json")) return null;
    let nativeServiceChildVerified=false;
    if(child){
      if(!attestation.nativeRoster||releaseInfo.scope!=='isolated-engineering-service-only'||!/^[a-f0-9]{64}$/.test(attestation.nativeServiceChildSha256??''))return null;
      const bytes=readPrivateNativeRoster(join(directory,'native-service-child.json'));
      if(!bytes||createHash('sha256').update(bytes).digest('hex')!==attestation.nativeServiceChildSha256)return null;
      const descriptor=JSON.parse(bytes.toString('utf8'));
      if(descriptor.version!==1||descriptor.appId!=='service'||descriptor.port!==3213||descriptor.gatewayPort!==3104||descriptor.run!==run||descriptor.sourceFingerprint!==attestation.sourceFingerprint||descriptor.fileManifestSha256!==attestation.fileManifestSha256||!/^[a-f0-9]{32}$/.test(descriptor.nonce??'')||child.ERPX_ENGINEERING_SERVICE_NONCE!==descriptor.nonce)return null;
      const entry=join(release,'apps/service/entry.mjs'),manifest=JSON.parse(readFileSync(manifestPath,'utf8'));
      const roots=[child.ERPX_RESOURCE_ROOT,child.ERPX_REPOSITORY_ROOT];
      if(realpathSync(entry)!==entry||process.argv[1]!==entry||realpathSync(process.argv[1])!==entry||roots.some(root=>!root||(root!==release&&root!==`${release}/`)||realpathSync(root)!==release)||!/^([a-f0-9]{64})$/.test(manifest['apps/service/entry.mjs']??'')||createHash('sha256').update(readFileSync(entry)).digest('hex')!==manifest['apps/service/entry.mjs'])return null;
      nativeServiceChildVerified=true;
    }
    return { directory, release, attestationPath, manifestPath, attestation, releaseInfo, digest: manifestDigest(manifestPath),nativeServiceChildVerified };
  } catch { return null; }
}
function application(db?: SqlDatabase): ReturnType<typeof createEngineeringVerificationApplication> { return createEngineeringVerificationApplication({ artifactVersion: artifact.softwareVersion, clock: { now: () => Date.now() }, readAttestation, nativeEnrollmentContext:()=>application().engineeringVerificationContext(process.env), repository: db ? {
  identity: roster => db.prepare(`SELECT current_database() name,current_user owner,${roster?nativeEngineeringIdentityPredicate(roster):"false"} native_match`).first<{ name: string; owner: string; native_match:boolean }>(),
  companyExists: id => engineeringCompanySource(db, id),
  prior: (companyId, run) => db.prepare("SELECT source_fingerprint,software_version,release_manifest_sha256 FROM engineering_verification_contexts WHERE company_id=? AND run_id=?").bind(companyId, run).first<{ source_fingerprint: string; software_version: string; release_manifest_sha256: string }>(),
  enroll: async (input, entries, now, expiresAt) => { await db.batch(entries.map(({ companyId, enabled }) => db.prepare(`INSERT INTO engineering_verification_contexts(company_id,run_id,source_fingerprint,software_version,release_manifest_sha256,enabled,expires_at,reason,created_at) VALUES(?,?,?,?,?,?,?,'Synthetic engineering verification only; all Company/provider certification remains pending',?) ON CONFLICT(company_id,run_id) DO UPDATE SET enabled=excluded.enabled,expires_at=excluded.expires_at`).bind(companyId, input.run, input.sourceFingerprint, input.softwareVersion, input.fileManifestSha256, enabled, expiresAt, now))); },
} : undefined }); }
export function engineeringVerificationContext(environment: NodeJS.ProcessEnv = process.env) { return application().engineeringVerificationContext(environment); }
export function isEngineeringVerificationOrigin(url: URL, context: EngineeringAttestation | null = engineeringVerificationContext()) { return acceptsOrigin(url, context); }
/** Exact roster members are restricted to Holding write databases. */
function nativeEngineeringIdentityPredicate(roster:NativeEngineeringRoster){
 const writes=roster.entries.filter(e=>/^H00[1-3]_DB$/.test(e.binding));
 const allowed=writes.map(e=>`(d.datname='erpx_dev_rust_${e.token}' AND d.oid=${e.oid}::oid AND shobj_description(d.oid,'pg_database')='erpx-rust-test:${e.token}')`).join(' OR ');
 return `/*erpx-native-engineering-fence*/ (current_user='erpx_test_admin' AND session_user='erpx_test_admin' AND floor(extract(epoch FROM pg_postmaster_start_time())*1000)::bigint=${roster.postmasterStartedMs} AND EXISTS(SELECT 1 FROM pg_database d JOIN pg_roles role ON role.rolname=current_user WHERE d.datname=current_database() AND pg_get_userbyid(d.datdba)='erpx_test_admin' AND NOT role.rolsuper AND NOT role.rolcreaterole AND NOT role.rolreplication AND NOT role.rolbypassrls AND role.rolcanlogin AND NOT has_table_privilege(current_user,'pg_catalog.pg_database','UPDATE') AND NOT has_table_privilege(current_user,'pg_catalog.pg_shdescription','UPDATE') AND NOT has_table_privilege(current_user,'pg_catalog.pg_database','DELETE') AND NOT has_table_privilege(current_user,'pg_catalog.pg_shdescription','DELETE') AND NOT has_table_privilege(current_user,'pg_catalog.pg_database','INSERT') AND NOT has_table_privilege(current_user,'pg_catalog.pg_shdescription','INSERT') AND NOT has_table_privilege(current_user,'pg_catalog.pg_database','TRUNCATE') AND NOT has_table_privilege(current_user,'pg_catalog.pg_shdescription','TRUNCATE') AND NOT EXISTS(SELECT 1 FROM aclexplode(COALESCE(d.datacl,acldefault('d',d.datdba))) acl WHERE acl.grantee=0 AND acl.privilege_type='CONNECT') AND (${allowed})))`;
}
/** Code-owned expression and sealed hexadecimal context; guards remain inside the native transaction. */
export function engineeringVerificationPredicate(companyExpression: string) {
  const context = engineeringVerificationContext();
  if (!context) return "FALSE";
  const identity=context.nativeRoster?nativeEngineeringIdentityPredicate(context.nativeRoster):`current_database() IN ('erpx_browser_${context.run}_h001_db','erpx_browser_${context.run}_h002_db','erpx_browser_${context.run}_h003_db')`;
  return `(${identity} AND EXISTS(SELECT 1 FROM engineering_verification_contexts engineering WHERE engineering.company_id=${companyExpression} AND engineering.run_id='${context.run}' AND engineering.source_fingerprint='${context.sourceFingerprint}' AND engineering.software_version='${context.softwareVersion}' AND engineering.release_manifest_sha256='${context.fileManifestSha256}' AND engineering.enabled AND engineering.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint))`;
}
export async function isolatedEngineeringVerification(db: SqlDatabase, companyId: string) { const predicate = engineeringVerificationPredicate("?"); return predicate !== "FALSE" && !!await db.prepare(`SELECT 1 WHERE ${predicate}`).bind(companyId).first(); }
export function enrollEngineeringVerification(db: SqlDatabase, raw: unknown) {
  if(engineeringVerificationContext()?.nativeRoster){const fenced=db as SqlDatabase & {hasNativeEngineeringFence?:()=>boolean;transaction?:<T>(action:(tx:SqlDatabase)=>Promise<T>)=>Promise<T>};if(typeof fenced.hasNativeEngineeringFence!=='function'||!fenced.hasNativeEngineeringFence()||typeof fenced.transaction!=='function')throw new Error('NATIVE_ENGINEERING_FENCE_REQUIRED');return fenced.transaction(tx=>application(tx).enrollEngineeringVerification(raw));}
  return application(db).enrollEngineeringVerification(raw);
}
