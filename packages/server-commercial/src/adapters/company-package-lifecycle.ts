import { flushFinanceDeferredEffects } from "@erpx/server-finance/public/finance-transactions";
import "server-only";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { PlatformCommercialLease } from "@erpx/server-identity_access/public/platform-commercial-authority";
import { commercialCompanies } from "@erpx/server-organization/public/commercial-companies";
import { ApiError } from "@erpx/server-runtime/public/http";
import { commercialPackages, requiresPackageSettlementReview } from "@erpx/server-commercial/domain/packages";
import type { PackageGrantBatch } from "@erpx/server-commercial/application/package-grant-batches";
import { packageGrantRevision, packageGrantStore, readCompanyPurchaseFacts, readCompanyPackageGrants } from "@erpx/server-commercial/adapters/package-grants";
import { ownerPackageObligations, withOwnerPackageObligations, hasOwnerPackageObligationSources } from "@erpx/server-commercial/adapters/owner-package-obligations";
import { packageReviewHash, packageReviewSigner } from "@erpx/server-commercial/adapters/package-review";

/** General package administration uses the same Commercial commands. Unknown
 * owner obligations stay closed; a provider purchase never creates IAM rights. */
export function companyPackageLifecycle(owner: PostgresDatabase, holdingId: string, lease: PlatformCommercialLease, secret: string, now = Date.now) {
  const signer = packageReviewSigner(secret, now, "package-lifecycle");
  const authorize = async (transaction: SqlDatabase, companyId: string) => {
    const actor = await lease.assertCurrent();
    if (!(await commercialCompanies(transaction)).some(company => company.id === companyId)) throw new ApiError(404, "COMPANY_NOT_FOUND", "Select an active Company.");
    return actor;
  };
  const store = packageGrantStore(owner, { authorize, obligations: (transaction, companyId, packageId) => ownerPackageObligations(owner, transaction, companyId, packageId), now });

  async function inUnit<Result>(input: PackageGrantBatch, run: (transaction: SqlDatabase) => Promise<Result>,
    finalize?: (transaction: SqlDatabase) => Promise<void>, replay = false) {
    // Versions/revision are rechecked by the normal service under its Company
    // lock. If this observation races another grant, the request becomes stale.
    const before = await readCompanyPackageGrants(owner, input.companyId);
    const ordinary = () => owner.transaction(async transaction => {
      const result = await run(transaction); await finalize?.(transaction); return result;
    });
    if (packageGrantRevision(input.companyId, before) !== input.expectedRevision) {
      // An identical concurrent Apply can commit between the first receipt
      // lookup and this read. Recover its receipt before stale/obligation gates.
      if (replay && await owner.prepare("SELECT 1 FROM company_package_change_sets WHERE id=?").bind(input.idempotencyKey).first()) return ordinary();
      throw new Error("STALE_PACKAGE_BATCH");
    }
    const settlement = input.changes.flatMap(change => {
      const previous = before.find(grant => grant.packageId === change.grant.packageId) ?? null;
      return requiresPackageSettlementReview(previous, change.grant) ? [{ packageId: change.grant.packageId, packageVersion: previous?.packageVersion ?? change.grant.packageVersion }] : [];
    });
    if (settlement.some(change => !hasOwnerPackageObligationSources(change)))
      throw new ApiError(409, "OWNER_OBLIGATIONS_REQUIRED", "The package owners must provide current settlement evidence before this change.");
    return settlement.length ? withOwnerPackageObligations(owner, settlement.map(change => ({ companyId: input.companyId, ...change })), run, finalize) : ordinary();
  }
  return {
    async overview(companyId: string) {
      await authorize(owner, companyId);
      const facts = await readCompanyPurchaseFacts(owner, companyId);
      await lease.assertCurrent();
      return { companyId, grants: facts.grants, history: facts.history, revision: packageGrantRevision(companyId, facts.grants), packages: commercialPackages,
        billingMode: "sandbox" as const, observedAt: now() };
    },
    async preview(input: PackageGrantBatch) {
      await authorize(owner, input.companyId);
      return inUnit(input, async transaction => {
        const comparison = await store.batch.previewIn(transaction, input);
        await authorize(transaction, input.companyId);
        return { comparison, request: input, review: signer.issue(lease.actor, holdingId, input, input.expectedRevision) };
      });
    },
    async apply(input: PackageGrantBatch, token: string) {
      if (lease.operation !== "apply") throw new ApiError(403, "PLATFORM_COMMERCIAL_REQUIRED", "A current Commercial management and MFA lease is required to apply packages.");
      const claims = signer.read(token, lease.actor, holdingId, input);
      await authorize(owner, input.companyId);
      // A committed receipt may be recovered even after its preview expires or
      // the current package advanced. The normal batch owns hash/replay checks.
      const committed = await owner.prepare("SELECT 1 FROM company_package_change_sets WHERE id=?").bind(input.idempotencyKey).first();
      let wrote = false;
      const run = async (transaction: SqlDatabase) => {
        const result = await store.batch.applyIn(transaction, input, async comparison => {
          signer.assertFresh(claims);
          if (packageGrantRevision(input.companyId, comparison.before) !== claims.authorityRevision)
            throw new ApiError(409, "PACKAGE_REVIEW_CHANGED", "Company packages changed. Preview them again.");
          await transaction.prepare(`INSERT INTO company_package_review_evidence(change_set_id,company_id,actor_id,assignment_id,role_id,role_revision,
            authority_revision,catalogue_hash,review_hash,reviewed_at,applied_at,comparison_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?::jsonb)`)
            .bind(input.idempotencyKey, input.companyId, lease.actor.userId, lease.actor.assignmentId, lease.actor.role, lease.actor.roleRevision,
              claims.authorityRevision, claims.catalogueHash, packageReviewHash(token), claims.issuedAt, now(),
              JSON.stringify({ source: { kind: "package-lifecycle", revision: claims.authorityRevision }, before: comparison.before, after: comparison.after })).run();
          wrote = true;
        });
        await flushFinanceDeferredEffects(transaction);
        await authorize(transaction, input.companyId);
        return result;
      };
      const finalize = async (transaction: SqlDatabase) => {
        // Owner final-source reads can wait too. Check provider/review freshness
        // after those reads, while the Holding unit can still roll back.
        if (wrote) signer.assertFresh(claims);
        await authorize(transaction, input.companyId);
      };
      return committed ? owner.transaction(async transaction => {
        const result = await run(transaction); await finalize(transaction); return result;
      }) : inUnit(input, run, finalize, true);
    },
  };
}
