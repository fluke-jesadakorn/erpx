import type { Clock } from "../../shared/application/ports";
import { acceptsEngineeringAttestation, engineeringEnrollmentSchema, type EngineeringAttestation, type EngineeringAttestationFacts, type EngineeringEnrollment } from "../domain/engineering-verification";
export type EngineeringEnvironment = Record<string, string | undefined>;
export type EngineeringRepository = {
  identity(): Promise<{ name: string; owner: string } | null>;
  companyExists(id: string): Promise<boolean>;
  prior(companyId: string, run: string): Promise<{ source_fingerprint: string; software_version: string; release_manifest_sha256: string } | null>;
  enroll(input: EngineeringEnrollment, entries: { companyId: string; enabled: boolean }[], now: number, expiresAt: number): Promise<void>;
};
export function createEngineeringVerificationApplication(ports: { artifactVersion: string; clock: Clock; readAttestation(directory: string, run: string): EngineeringAttestationFacts | null; repository?: EngineeringRepository }) {
  function engineeringVerificationContext(environment: EngineeringEnvironment): EngineeringAttestation | null {
    const run = environment.ERPX_ENGINEERING_VERIFICATION_RUN;
    if (!run || !/^[a-f0-9]{12}$/.test(run) || environment.ERPX_ENVIRONMENT !== "local" || environment.PORT !== "3104" || environment.ERPX_BROWSER_SIMULATION_PORT !== "3104") return null;
    try {
      const facts = ports.readAttestation(environment.ERPX_DATA_DIR ?? "", run);
      return facts && acceptsEngineeringAttestation(run, ports.artifactVersion, facts) ? facts.attestation : null;
    } catch { return null; }
  }
  async function enrollEngineeringVerification(raw: unknown) {
    const input = engineeringEnrollmentSchema.parse(raw);
    if (input.softwareVersion !== `release-${input.sourceFingerprint}`) throw new Error("ENGINEERING_SOURCE_BUILD_MISMATCH");
    const repository = ports.repository;
    if (!repository) throw new Error("ENGINEERING_REPOSITORY_REQUIRED");
    const expected = [`erpx_browser_${input.run}_h001_db`, `erpx_browser_${input.run}_h002_db`, `erpx_browser_${input.run}_h003_db`], database = await repository.identity();
    if (!database || !expected.includes(database.name) || database.owner !== "erpx_test_admin") throw new Error("EXACT_ENGINEERING_ADMIN_DATABASE_REQUIRED");
    const entries = [...input.companyIds.map(companyId => ({ companyId, enabled: true })), ...input.disabledCompanyIds.map(companyId => ({ companyId, enabled: false }))];
    if (new Set(entries.map(entry => entry.companyId)).size !== entries.length) throw new Error("ENGINEERING_COMPANY_DUPLICATED");
    const now = ports.clock.now(), expiresAt = input.expiresAt ?? now + 12 * 60 * 60_000;
    if (expiresAt <= now || expiresAt > now + 24 * 60 * 60_000) throw new Error("ENGINEERING_EXPIRY_REQUIRED");
    for (const { companyId } of entries) {
      if (!await repository.companyExists(companyId)) throw new Error("ENGINEERING_COMPANY_NOT_FOUND");
      const prior = await repository.prior(companyId, input.run);
      if (prior && (prior.source_fingerprint !== input.sourceFingerprint || prior.software_version !== input.softwareVersion || prior.release_manifest_sha256 !== input.fileManifestSha256)) throw new Error("ENGINEERING_RUN_BUILD_CHANGED");
    }
    await repository.enroll(input, entries, now, expiresAt);
    return { scope: "isolated-engineering" as const, run: input.run, sourceFingerprint: input.sourceFingerprint, softwareVersion: input.softwareVersion, fileManifestSha256: input.fileManifestSha256, expiresAt, companyIds: input.companyIds, disabledCompanyIds: input.disabledCompanyIds };
  }
  return { engineeringVerificationContext, enrollEngineeringVerification };
}
