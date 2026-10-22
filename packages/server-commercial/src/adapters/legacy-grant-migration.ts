import "server-only";
import { randomUUID } from "node:crypto";
import {flushFinanceDeferredEffects} from "@erpx/server-finance/public/finance-transactions";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import { ApiError } from "@erpx/server-runtime/public/http";
import { commercialCompanies } from "@erpx/server-organization/public/commercial-companies";
import { withLegacyModuleAccessLease, type LegacyModuleAccessLease } from "@erpx/server-identity_access/public/legacy-module-access";
import type { PlatformCommercialLease } from "@erpx/server-identity_access/public/platform-commercial-authority";
import { legacyPackagePreview } from "@erpx/server-commercial/domain/legacy-packages";
import { packageReviewRequest } from "@erpx/server-commercial/domain/package-review";
import { packageGrantRevision, packageGrantStore, readCompanyPackageGrants } from "@erpx/server-commercial/adapters/package-grants";
import type { PackageGrantBatch } from "@erpx/server-commercial/application/package-grant-batches";
import { packageReviewSigner } from "@erpx/server-commercial/adapters/package-review";
import { packageReviewHash } from "@erpx/server-commercial/adapters/package-review";

export function legacyGrantMigration(db: PostgresDatabase, holdingId: string, lease: PlatformCommercialLease, secret: string, now = Date.now) {
  const signer = packageReviewSigner(secret, now);
  const authorize = async (database: Parameters<typeof readCompanyPackageGrants>[0], companyId: string) => {
    const actor = await lease.assertCurrent();
    if (!(await commercialCompanies(database)).some(company => company.id === companyId)) throw new ApiError(404, "COMPANY_NOT_FOUND", "Select an active Company.");
    return actor;
  };
  const unavailableObligations = async (): Promise<number> => { throw new ApiError(409, "OWNER_OBLIGATIONS_REQUIRED", "Owner obligation evidence is required before deactivation."); };
  return {
    async preview(companyId: string, reason: string, evidence: string) {
      return withLegacyModuleAccessLease(db, companyId, async (tx, readSource) => {
        // Before the first snapshot: newly inserted roles/assignments and
        // appointment changes are part of this exact reviewed source.
        await authorize(tx, companyId);
        const source = await readSource(), facts = source.facts, before = await readCompanyPackageGrants(tx, companyId);
        const instant = now(), comparison = legacyPackagePreview(facts, before, { startsAt: instant, now: instant, reason, evidence });
        if (comparison.blockers.length || !comparison.changes.length) { await source.assertCurrent(); return { comparison, request: null, review: null }; }
        const request: PackageGrantBatch = { companyId, expectedRevision: packageGrantRevision(companyId, before),
          changes: comparison.changes, idempotencyKey: randomUUID() };
        await packageGrantStore(db, { authorize, obligations: unavailableObligations, now }).batch.previewIn(tx, request);
        await source.assertCurrent();
        return { comparison, request, review: signer.issue(lease.actor, holdingId, request, facts.revision) };
      }, now);
    },
    async apply(request: PackageGrantBatch, token: string) {
      if (lease.operation !== "apply") throw new ApiError(403, "PLATFORM_COMMERCIAL_REQUIRED", "A current Commercial management and MFA lease is required to apply packages.");
      const claims = signer.read(token, lease.actor, holdingId, request);
      return withLegacyModuleAccessLease(db, request.companyId, async (transaction, readSource) => {
        let source: LegacyModuleAccessLease | undefined;
        const guarded = packageGrantStore(db, { authorize, obligations: unavailableObligations, now });
        const result = await guarded.batch.applyIn(transaction, request, async preview => {
          signer.assertFresh(claims);
          source = await readSource();
          if (source.facts.revision !== claims.authorityRevision) throw new ApiError(409, "STALE_LEGACY_PREVIEW", "Legacy access changed. Preview this Company again.");
          // A saved review can outlive a deployment of the coverage rules while
          // the published Company witnesses and package catalogue stay equal.
          // New commits must still match today's normal owner-derived plan.
          // This callback is skipped for a committed exact receipt replay.
          const reviewed = request.changes[0].grant;
          const current = legacyPackagePreview(source.facts, preview.before, { startsAt: reviewed.startsAt,
            now: now(), reason: reviewed.reason, evidence: reviewed.evidence });
          if (current.blockers.length || packageReviewHash(JSON.stringify(packageReviewRequest({ ...request, changes: current.changes }))) !== claims.requestHash)
            throw new ApiError(409, "STALE_LEGACY_PREVIEW", "Legacy coverage changed. Preview this Company again.");
          await transaction.prepare(`INSERT INTO company_package_review_evidence(change_set_id,company_id,actor_id,assignment_id,role_id,role_revision,
          authority_revision,catalogue_hash,review_hash,reviewed_at,applied_at,comparison_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?::jsonb)`)
          .bind(request.idempotencyKey, request.companyId, lease.actor.userId, lease.actor.assignmentId, lease.actor.role, lease.actor.roleRevision,
            claims.authorityRevision, claims.catalogueHash, packageReviewHash(token), claims.issuedAt, now(), JSON.stringify({ source: source.facts, before: preview.before, after: preview.after })).run();
        });
        // These checks run after every grant, audit and batch receipt but before
        // the Holding commit. A committed replay needs current provider authority,
        // not renewed eligibility of historical license witnesses.
        await flushFinanceDeferredEffects(transaction);
        await authorize(transaction, request.companyId);
        if (source) { await source.assertCurrent(); signer.assertFresh(claims); }
        // Source verification can wait under its original leases. Recheck the
        // current provider after that work while the Holding unit can roll back.
        await authorize(transaction, request.companyId);
        return result;
      }, now);
    },
  };
}
