import "server-only";
import {createHash} from "node:crypto";
import {readFileSync,realpathSync} from "node:fs";
import {join,resolve} from "node:path";
import artifact from "@/data/release-artifact.json";
import type {SqlDatabase} from "./runtime/types";
import {z} from "zod";

type Attestation={scope:"isolated-engineering";run:string;sourceFingerprint:string;softwareVersion:string;fileManifestSha256:string};
/** The sealed local candidate is the only compiled runtime allowed to use HTTP. */
export function isEngineeringVerificationOrigin(url:URL,context:Attestation|null=engineeringVerificationContext()) {
 return Boolean(context)&&url.protocol==='http:'&&url.port==='3104'&&
  (['127.0.0.1','localhost'].includes(url.hostname)||/^dev-[a-z0-9-]+\.localhost$/.test(url.hostname));
}
/** This context authorizes fixtures, never legal/provider or business certification. */
export function engineeringVerificationContext(environment:NodeJS.ProcessEnv=process.env):Attestation|null {
 const run=environment.ERPX_ENGINEERING_VERIFICATION_RUN;
 if(!run||!/^[a-f0-9]{12}$/.test(run)||environment.ERPX_ENVIRONMENT!=="local"||environment.PORT!=="3104"||environment.ERPX_BROWSER_SIMULATION_PORT!=="3104")return null;
 try{
  const directory=realpathSync(resolve(environment.ERPX_DATA_DIR??""));
  if(!directory.endsWith(`/.runtime/browser/${run}`))return null;
  const release=realpathSync(join(directory,"release"));
  if(release!==join(directory,"release"))return null;
  const attestationPath=realpathSync(join(directory,"engineering-attestation.json"));
  if(attestationPath!==join(directory,"engineering-attestation.json"))return null;
  const attestation=JSON.parse(readFileSync(attestationPath,"utf8")) as Attestation;
  const releaseInfo=JSON.parse(readFileSync(join(release,"release.json"),"utf8")) as Attestation;
  const manifestPath=realpathSync(join(release,"release-files.json"));
  if(manifestPath!==join(release,"release-files.json"))return null;
  const digest=createHash("sha256").update(readFileSync(manifestPath)).digest("hex");
  if(attestation.scope!=="isolated-engineering"||attestation.run!==run||!/^[a-f0-9]{64}$/.test(attestation.sourceFingerprint)||
    attestation.softwareVersion!==`release-${attestation.sourceFingerprint}`||attestation.softwareVersion!==artifact.softwareVersion||
    releaseInfo.sourceFingerprint!==attestation.sourceFingerprint||releaseInfo.softwareVersion!==attestation.softwareVersion||
    digest!==attestation.fileManifestSha256||releaseInfo.fileManifestSha256!==digest)return null;
  return attestation;
 }catch{return null;}
}

/** All interpolated values are validated hexadecimal identifiers; company expression is code-owned. */
export function engineeringVerificationPredicate(companyExpression:string) {
 const context=engineeringVerificationContext();
 if(!context)return "FALSE";
 return `(current_database() IN ('erpx_browser_${context.run}_h001_db','erpx_browser_${context.run}_h002_db','erpx_browser_${context.run}_h003_db')
  AND EXISTS(SELECT 1 FROM engineering_verification_contexts engineering WHERE engineering.company_id=${companyExpression}
   AND engineering.run_id='${context.run}' AND engineering.source_fingerprint='${context.sourceFingerprint}' AND engineering.software_version='${context.softwareVersion}'
   AND engineering.release_manifest_sha256='${context.fileManifestSha256}' AND engineering.enabled
   AND engineering.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint))`;
}
export async function isolatedEngineeringVerification(db:SqlDatabase,companyId:string) {
 const predicate=engineeringVerificationPredicate("?");
 return predicate!=="FALSE"&&!!await db.prepare(`SELECT 1 WHERE ${predicate}`).bind(companyId).first();
}

/** Called only by the isolated fixture harness, with its administrator connection. */
export async function enrollEngineeringVerification(db:SqlDatabase,raw:unknown) {
 const input=z.object({run:z.string().regex(/^[a-f0-9]{12}$/),sourceFingerprint:z.string().regex(/^[a-f0-9]{64}$/),softwareVersion:z.string(),fileManifestSha256:z.string().regex(/^[a-f0-9]{64}$/),companyIds:z.array(z.string().min(1)).max(100),disabledCompanyIds:z.array(z.string().min(1)).max(100).default([]),expiresAt:z.number().int().positive().optional()}).parse(raw);
 if(input.softwareVersion!==`release-${input.sourceFingerprint}`)throw new Error("ENGINEERING_SOURCE_BUILD_MISMATCH");
 const expected=[`erpx_browser_${input.run}_h001_db`,`erpx_browser_${input.run}_h002_db`,`erpx_browser_${input.run}_h003_db`];
 const database=await db.prepare("SELECT current_database() name,current_user owner").first<{name:string;owner:string}>();
 if(!database||!expected.includes(database.name)||database.owner!=="erpx_test_admin")throw new Error("EXACT_ENGINEERING_ADMIN_DATABASE_REQUIRED");
 const entries=[...input.companyIds.map(companyId=>({companyId,enabled:true})),...input.disabledCompanyIds.map(companyId=>({companyId,enabled:false}))];
 if(new Set(entries.map(entry=>entry.companyId)).size!==entries.length)throw new Error("ENGINEERING_COMPANY_DUPLICATED");
 const now=Date.now(),expiresAt=input.expiresAt??now+12*60*60_000;
 if(expiresAt<=now||expiresAt>now+24*60*60_000)throw new Error("ENGINEERING_EXPIRY_REQUIRED");
 for(const {companyId} of entries){
  if(!await db.prepare("SELECT 1 FROM organization_nodes WHERE id=? AND kind='company'").bind(companyId).first())throw new Error("ENGINEERING_COMPANY_NOT_FOUND");
  const prior=await db.prepare("SELECT source_fingerprint,software_version,release_manifest_sha256 FROM engineering_verification_contexts WHERE company_id=? AND run_id=?").bind(companyId,input.run).first<{source_fingerprint:string;software_version:string;release_manifest_sha256:string}>();
  if(prior&&(prior.source_fingerprint!==input.sourceFingerprint||prior.software_version!==input.softwareVersion||prior.release_manifest_sha256!==input.fileManifestSha256))throw new Error("ENGINEERING_RUN_BUILD_CHANGED");
 }
 await db.batch(entries.map(({companyId,enabled})=>db.prepare(`INSERT INTO engineering_verification_contexts(company_id,run_id,source_fingerprint,software_version,release_manifest_sha256,enabled,expires_at,reason,created_at)
  VALUES(?,?,?,?,?,?,?,'Synthetic engineering verification only; all Company/provider certification remains pending',?)
  ON CONFLICT(company_id,run_id) DO UPDATE SET enabled=excluded.enabled,expires_at=excluded.expires_at`).bind(companyId,input.run,input.sourceFingerprint,input.softwareVersion,input.fileManifestSha256,enabled,expiresAt,now)));
 return {scope:"isolated-engineering" as const,run:input.run,sourceFingerprint:input.sourceFingerprint,softwareVersion:input.softwareVersion,fileManifestSha256:input.fileManifestSha256,expiresAt,companyIds:input.companyIds,disabledCompanyIds:input.disabledCompanyIds};
}
