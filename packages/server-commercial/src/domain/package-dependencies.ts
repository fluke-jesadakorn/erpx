import { packageDefinition, type CompanyPackageGrant } from "@erpx/server-commercial/domain/packages";

export function validatePackageDependencies(grant: CompanyPackageGrant, all: readonly CompanyPackageGrant[], now: number) {
  const definition = packageDefinition(grant.packageId, grant.packageVersion);
  if (!definition) throw new Error("UNKNOWN_PACKAGE_VERSION");
  const dependents = all.filter(child=>child.companyId===grant.companyId && child.state!=="inactive" &&
    packageDefinition(child.packageId,child.packageVersion)?.dependencies.includes(grant.packageId));
  if (grant.state === "inactive") {
    if (dependents.length) throw new Error("PACKAGE_DEPENDENCY_IN_USE");
    return;
  }
  for (const dependency of definition.dependencies) {
    const current = all.find(candidate => candidate.companyId === grant.companyId && candidate.packageId === dependency);
    if (!current || !packageDefinition(current.packageId, current.packageVersion) || current.state === "inactive" || current.startsAt > grant.startsAt ||
      grant.state === "active" && (current.state !== "active" || current.endsAt !== null &&
        (grant.endsAt === null || current.endsAt < grant.endsAt || current.endsAt <= now))) throw new Error("PACKAGE_DEPENDENCY_REQUIRED");
  }
  if (definition.kind === "base" && dependents.some(child=>child.startsAt<grant.startsAt ||
    grant.state === "active" && child.state === "active" && grant.endsAt !== null &&
      (child.endsAt === null || child.endsAt > grant.endsAt))) throw new Error("PACKAGE_DEPENDENCY_PERIOD_CONFLICT");
}
