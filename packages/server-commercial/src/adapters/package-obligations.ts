import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { packageObligationModules, type PackageObligationObservation } from "@erpx/server-commercial/domain/package-obligations";

export type PackageObligationInput = { companyId: string; packageId: string; packageVersion: number; now: number };
export type PackageObligationReader = (database: SqlDatabase, input: PackageObligationInput & { moduleId: string }) => Promise<PackageObligationObservation>;
/** Missing, stale or incorrectly scoped owner evidence always blocks inactivation. */
export async function readPackageObligations(database: SqlDatabase, input: PackageObligationInput, readers: Readonly<Record<string, PackageObligationReader>>) {
  const observations: PackageObligationObservation[] = [];
  for (const moduleId of packageObligationModules(input.packageId, input.packageVersion)) {
    const reader = readers[moduleId];
    if (!reader) throw new Error(`PACKAGE_OBLIGATION_READER_REQUIRED:${moduleId}`);
    const observation = await reader(database, { ...input, moduleId });
    if (observation.companyId !== input.companyId || observation.packageId !== input.packageId || observation.moduleId !== moduleId ||
      !observation.revision || observation.observedAt !== input.now || !Number.isSafeInteger(observation.outstanding) || observation.outstanding < 0)
      throw new Error(`INVALID_PACKAGE_OBLIGATION_EVIDENCE:${moduleId}`);
    observations.push(observation);
  }
  const total = observations.reduce((sum, observation) => sum + observation.outstanding, 0);
  if (!Number.isSafeInteger(total)) throw new Error("INVALID_PACKAGE_OBLIGATION_TOTAL");
  return { total, observations };
}
