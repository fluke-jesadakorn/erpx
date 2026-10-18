import { packageGrantCommands, type GrantChange, type PackageGrantUnit } from "./package-grants";
import { canonicalPackageGrant, packageDefinition, type CompanyPackageGrant } from "../domain/packages";

export type PackageGrantBatch = {
  companyId: string;
  expectedRevision: string;
  changes: Omit<GrantChange, "idempotencyKey">[];
  idempotencyKey: string;
};
export type PackageGrantBatchPreview = { companyId: string; expectedRevision: string; before: CompanyPackageGrant[]; after: CompanyPackageGrant[]; changes: Omit<GrantChange, "idempotencyKey">[] };
export type PackageGrantBatchUnit = {
  grants: PackageGrantUnit;
  all(companyId: string): Promise<CompanyPackageGrant[]>;
  revision(companyId: string, grants: CompanyPackageGrant[]): string;
  previewDependencies(grant: CompanyPackageGrant, prospective: CompanyPackageGrant[]): Promise<void>;
  receipt(key: string): Promise<{ hash: string; result: PackageGrantBatchPreview } | null>;
  saveReceipt(key: string, hash: string, actorId: string, result: PackageGrantBatchPreview): Promise<void>;
};

export function packageGrantBatchCommands(ports: {
  unit(): PackageGrantBatchUnit;
  transaction<T>(action: (unit: PackageGrantBatchUnit) => Promise<T>): Promise<T>;
  hash(value: string): string;
}) {
  function normalized(input: PackageGrantBatch) {
    if (!input.companyId || !/^[a-f0-9]{64}$/.test(input.expectedRevision) || !/^[A-Za-z0-9_-]{16,128}$/.test(input.idempotencyKey) || !input.changes.length || input.changes.length > 100) throw new Error("INVALID_PACKAGE_BATCH");
    const seen = new Set<string>();
    const changes = input.changes.map(change => {
      if (change.grant.companyId !== input.companyId || seen.has(change.grant.packageId)) throw new Error("INVALID_PACKAGE_BATCH");
      seen.add(change.grant.packageId);
      return { expectedVersion: change.expectedVersion, grant: canonicalPackageGrant(change.grant) };
    }).sort((a, b) => {
      const first = packageDefinition(a.grant.packageId, a.grant.packageVersion), second = packageDefinition(b.grant.packageId, b.grant.packageVersion);
      return Number(first?.kind === "addon") - Number(second?.kind === "addon") || a.grant.packageId.localeCompare(b.grant.packageId);
    });
    return { companyId: input.companyId, expectedRevision: input.expectedRevision, changes };
  }
  async function inspect(unit: PackageGrantBatchUnit, input: ReturnType<typeof normalized>) {
    const before = await unit.all(input.companyId);
    if (unit.revision(input.companyId, before) !== input.expectedRevision) throw new Error("STALE_PACKAGE_BATCH");
    const prospective = new Map(before.map(grant => [grant.packageId, grant]));
    for (const change of input.changes) prospective.set(change.grant.packageId, change.grant);
    const after = [...prospective.values()].sort((a, b) => a.packageId.localeCompare(b.packageId));
    // Reuse the normal Commercial preview policy, supplying the prospective
    // dependency set without writing any row or pretending a base already exists.
    const preview = packageGrantCommands({ unit: () => ({ ...unit.grants,
      dependencies: grant => unit.previewDependencies(grant, after),
    }), transaction: async () => { throw new Error("PREVIEW_CANNOT_WRITE"); }, hash: ports.hash });
    for (const change of input.changes) await preview.preview({ ...change, idempotencyKey: "preview-only-no-write" });
    return { ...input, before, after };
  }
  return {
    async preview(input: PackageGrantBatch) {
      const request = normalized(input), unit = ports.unit();
      await unit.grants.authorize(request.companyId);
      return inspect(unit, request);
    },
    async apply(input: PackageGrantBatch, verifyPreview: (preview: PackageGrantBatchPreview) => Promise<void>) {
      const request = normalized(input), hash = ports.hash(JSON.stringify(request));
      return ports.transaction(async unit => {
        await unit.grants.lock(`receipt:${input.idempotencyKey}`);
        await unit.grants.lock(`company:${request.companyId}`);
        const actor = await unit.grants.authorize(request.companyId);
        const old = await unit.receipt(input.idempotencyKey);
        if (old) { if (old.hash !== hash) throw new Error("IDEMPOTENCY_CONFLICT"); return old.result; }
        const preview = await inspect(unit, request);
        await verifyPreview(preview);
        // Each grant and its immutable owner audit still use the normal service.
        // The outer unit makes the whole reviewed Company change atomic.
        const grants = { ...unit.grants, dependencies: (grant: CompanyPackageGrant) => unit.previewDependencies(grant, preview.after) };
        const commands = packageGrantCommands({ unit: () => grants, transaction: action => action(grants), hash: ports.hash });
        for (const change of request.changes) await commands.apply({ ...change,
          idempotencyKey: ports.hash(`${input.idempotencyKey}:${change.grant.packageId}`) });
        await unit.grants.authorize(request.companyId);
        await unit.saveReceipt(input.idempotencyKey, hash, actor.userId, preview);
        return preview;
      });
    },
  };
}
