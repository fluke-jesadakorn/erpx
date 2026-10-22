import { requiresPackageSettlementReview, validateGrantChange, type CompanyPackageGrant } from "@erpx/server-commercial/domain/packages";

export type GrantChange = { grant: CompanyPackageGrant; expectedVersion: number; idempotencyKey: string };
export type PackageGrantUnit = {
  /** Must recheck current provider authority in the command's protected unit. */
  authorize(companyId: string): Promise<{ userId: string }>;
  lock(companyId: string): Promise<void>;
  current(companyId: string, packageId: string): Promise<CompanyPackageGrant | null>;
  receipt(key: string): Promise<{ hash: string; grant: CompanyPackageGrant } | null>;
  obligations(companyId: string, packageId: string): Promise<number>;
  dependencies(grant: CompanyPackageGrant): Promise<void>;
  save(input: GrantChange, current: CompanyPackageGrant | null, actorId: string, hash: string): Promise<void>;
};
export function packageGrantCommands(ports: { unit(): PackageGrantUnit; transaction<T>(action: (unit: PackageGrantUnit) => Promise<T>): Promise<T>; hash(value: string): string }) {
  async function inspect(unit: PackageGrantUnit, input: GrantChange) {
    const actor = await unit.authorize(input.grant.companyId);
    const current = await unit.current(input.grant.companyId, input.grant.packageId);
    // Reject stale/invalid requests before acquiring an owner observation. The
    // real count remains mandatory for every change that can withdraw settlement.
    validateGrantChange(current, input.grant, input.expectedVersion, 0);
    const obligations = requiresPackageSettlementReview(current,input.grant) ? await unit.obligations(input.grant.companyId, input.grant.packageId) : 0;
    validateGrantChange(current, input.grant, input.expectedVersion, obligations);
    await unit.dependencies(input.grant);
    return { actor, current };
  }
  return {
    async preview(input: GrantChange) {
      const { current } = await inspect(ports.unit(), input);
      return { before: current, after: input.grant, expectedVersion: input.expectedVersion };
    },
    async apply(input: GrantChange) {
      if (!/^[A-Za-z0-9_-]{16,128}$/.test(input.idempotencyKey)) throw new Error("INVALID_IDEMPOTENCY_KEY");
      const hash = ports.hash(JSON.stringify({ companyId: input.grant.companyId, packageId: input.grant.packageId, packageVersion: input.grant.packageVersion, state: input.grant.state, startsAt: input.grant.startsAt, endsAt: input.grant.endsAt, version: input.grant.version, reason: input.grant.reason, evidence: input.grant.evidence, expectedVersion: input.expectedVersion }));
      return ports.transaction(async unit => {
        // Lock receipt identity before Company identity; replays across Companies cannot race.
        await unit.lock(`receipt:${input.idempotencyKey}`);
        await unit.lock(`company:${input.grant.companyId}`);
        const actor = await unit.authorize(input.grant.companyId);
        const replay = await unit.receipt(input.idempotencyKey);
        if (replay) {
          if (replay.hash !== hash) throw new Error("IDEMPOTENCY_CONFLICT");
          return replay.grant;
        }
        const { current } = await inspect(unit, input);
        await unit.save(input, current, actor.userId, hash);
        return input.grant;
      });
    },
  };
}
