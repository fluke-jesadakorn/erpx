import "server-only";
import { memoizeSyncInRequest } from "../../runtime/public/runtime/request-memo";
import { engineeringCompanySource } from "../../organization/public/engineering-company-source";
import { createHash } from "node:crypto";
import { readFileSync, realpathSync } from "node:fs";
import { join, resolve } from "node:path";
import artifact from "@/data/release-artifact.json";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import { createEngineeringVerificationApplication } from "../application/engineering-verification";
import { isEngineeringVerificationOrigin as acceptsOrigin, type EngineeringAttestation, type EngineeringAttestationFacts } from "../domain/engineering-verification";

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

function readAttestation(input: string, run: string): EngineeringAttestationFacts | null {
  try {
    const directory = realpathSync(resolve(input)); if (!directory.endsWith(`/.runtime/browser/${run}`)) return null;
    const release = realpathSync(join(directory, "release")); if (release !== join(directory, "release")) return null;
    const attestationPath = realpathSync(join(directory, "engineering-attestation.json")); if (attestationPath !== join(directory, "engineering-attestation.json")) return null;
    const attestation = JSON.parse(readFileSync(attestationPath, "utf8")), releaseInfo = JSON.parse(readFileSync(join(release, "release.json"), "utf8"));
    const manifestPath = realpathSync(join(release, "release-files.json")); if (manifestPath !== join(release, "release-files.json")) return null;
    return { directory, release, attestationPath, manifestPath, attestation, releaseInfo, digest: manifestDigest(manifestPath) };
  } catch { return null; }
}
function application(db?: SqlDatabase) { return createEngineeringVerificationApplication({ artifactVersion: artifact.softwareVersion, clock: { now: () => Date.now() }, readAttestation, repository: db ? {
  identity: () => db.prepare("SELECT current_database() name,current_user owner").first<{ name: string; owner: string }>(),
  companyExists: id => engineeringCompanySource(db, id),
  prior: (companyId, run) => db.prepare("SELECT source_fingerprint,software_version,release_manifest_sha256 FROM engineering_verification_contexts WHERE company_id=? AND run_id=?").bind(companyId, run).first<{ source_fingerprint: string; software_version: string; release_manifest_sha256: string }>(),
  enroll: async (input, entries, now, expiresAt) => { await db.batch(entries.map(({ companyId, enabled }) => db.prepare(`INSERT INTO engineering_verification_contexts(company_id,run_id,source_fingerprint,software_version,release_manifest_sha256,enabled,expires_at,reason,created_at) VALUES(?,?,?,?,?,?,?,'Synthetic engineering verification only; all Company/provider certification remains pending',?) ON CONFLICT(company_id,run_id) DO UPDATE SET enabled=excluded.enabled,expires_at=excluded.expires_at`).bind(companyId, input.run, input.sourceFingerprint, input.softwareVersion, input.fileManifestSha256, enabled, expiresAt, now))); },
} : undefined }); }
export function engineeringVerificationContext(environment: NodeJS.ProcessEnv = process.env) { return application().engineeringVerificationContext(environment); }
export function isEngineeringVerificationOrigin(url: URL, context: EngineeringAttestation | null = engineeringVerificationContext()) { return acceptsOrigin(url, context); }
/** Code-owned expression and sealed hexadecimal context; guards remain inside the native transaction. */
export function engineeringVerificationPredicate(companyExpression: string) {
  const context = engineeringVerificationContext();
  if (!context) return "FALSE";
  return `(current_database() IN ('erpx_browser_${context.run}_h001_db','erpx_browser_${context.run}_h002_db','erpx_browser_${context.run}_h003_db') AND EXISTS(SELECT 1 FROM engineering_verification_contexts engineering WHERE engineering.company_id=${companyExpression} AND engineering.run_id='${context.run}' AND engineering.source_fingerprint='${context.sourceFingerprint}' AND engineering.software_version='${context.softwareVersion}' AND engineering.release_manifest_sha256='${context.fileManifestSha256}' AND engineering.enabled AND engineering.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint))`;
}
export async function isolatedEngineeringVerification(db: SqlDatabase, companyId: string) { const predicate = engineeringVerificationPredicate("?"); return predicate !== "FALSE" && !!await db.prepare(`SELECT 1 WHERE ${predicate}`).bind(companyId).first(); }
export function enrollEngineeringVerification(db: SqlDatabase, raw: unknown) { return application(db).enrollEngineeringVerification(raw); }
