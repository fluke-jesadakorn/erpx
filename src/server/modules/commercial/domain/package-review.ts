import { canonicalPackageGrant, commercialPackages, type CompanyPackageGrant } from "./packages";
type ReviewedBatch = { companyId: string; expectedRevision: string; idempotencyKey: string; changes: { expectedVersion: number; grant: CompanyPackageGrant }[] };

export type PackageReviewActor = { userId: string; sessionId: string; assignmentId: string; role: string; roleRevision: number };
export type PackageReviewClaims = { version: 1; holdingId: string; companyId: string; actor: PackageReviewActor; requestHash: string;
  authorityRevision: string; catalogueHash: string; issuedAt: number; expiresAt: number };

export function packageReviewRequest(input: ReviewedBatch) {
  return { companyId: input.companyId, expectedRevision: input.expectedRevision, idempotencyKey: input.idempotencyKey,
    changes: input.changes.map(change => ({ expectedVersion: change.expectedVersion, grant: canonicalPackageGrant(change.grant) })).sort((a, b) => a.grant.packageId.localeCompare(b.grant.packageId)) };
}
export function packageCatalogueSnapshot() { return JSON.stringify(commercialPackages); }
export function samePackageReviewActor(a: PackageReviewActor, b: PackageReviewActor) {
  return a.userId === b.userId && a.sessionId === b.sessionId && a.assignmentId === b.assignmentId && a.role === b.role && a.roleRevision === b.roleRevision;
}
