import { purchasedCapability, type CapabilityPurpose, type CompanyPackageGrant, type CompanyPackageHistoryRight, type PackageDecision } from "@erpx/server-commercial/domain/packages";

export type CapabilityGate = { allowed: boolean; revision: string };
export type EffectiveCapabilityInput = {
  companyId: string;
  capability: string;
  purpose: CapabilityPurpose;
  grants: readonly CompanyPackageGrant[];
  history?: readonly CompanyPackageHistoryRight[];
  now: number;
  release: CapabilityGate;
  authority: CapabilityGate;
  record: CapabilityGate;
};
export type EffectiveCapabilityDecision = {
  allowed: boolean;
  code: PackageDecision["code"] | "RELEASE_NOT_READY" | "AUTHORITY_REQUIRED" | "RECORD_DENIED";
  revisions: { release: string; authority: string; record: string };
};

/** Every owner supplies fresh, scoped gates. A purchase never supplies authority. */
export function effectiveCapabilityDecision(input: EffectiveCapabilityInput): EffectiveCapabilityDecision {
  const purchased = purchasedCapability(input.grants, input.companyId, input.capability, input.purpose, input.now, input.history);
  const revisions = { release: input.release.revision, authority: input.authority.revision, record: input.record.revision };
  const code = !purchased.allowed ? purchased.code
    : !input.release.allowed || !input.release.revision ? "RELEASE_NOT_READY"
      : !input.authority.allowed || !input.authority.revision ? "AUTHORITY_REQUIRED"
        : !input.record.allowed || !input.record.revision ? "RECORD_DENIED" : "GRANTED";
  return { allowed: code === "GRANTED", code, revisions };
}

/** Stable across row order and wall-clock ticks; changes at starts/expiry boundaries. */
export function capabilityRevisionSource(companyId: string, grants: readonly CompanyPackageGrant[], now: number, history: readonly CompanyPackageHistoryRight[] = []) {
  return JSON.stringify({ companyId, grants: grants.filter(grant => grant.companyId === companyId)
    .map(grant => [grant.packageId, grant.packageVersion, grant.version, grant.state, grant.startsAt, grant.endsAt,
      grant.startsAt > now ? "future" : grant.endsAt !== null && grant.endsAt <= now ? "expired" : "current"])
    .sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b))),
    history: history.filter(right => right.companyId === companyId)
      .map(right => [right.packageId, right.packageVersion, right.sourceGrantVersion, right.sourceEventId, right.retainedAt, right.retainedAt > now ? "future" : "retained"])
      .sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b))) });
}
