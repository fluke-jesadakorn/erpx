// Generated from the Rust Service Commercial contracts. Do not edit by hand.
// Regenerate: bun scripts/docs/generate-commercial-service-contracts.ts
export type PackageDefinition = { id: string, version: number, productId: string, kind: string, visibleCapabilities: Array<string>, includedCapabilities: Array<string>, dependencies: Array<string>, };
export type CompanyPackageGrant = { companyId: string, packageId: string, packageVersion: number, state: string, startsAt: number, endsAt: number | null, version: number, reason: string, evidence: string, };
export type CompanyPackageHistoryRight = { companyId: string, packageId: string, packageVersion: number, sourceGrantVersion: number, sourceEventId: string, retainedAt: number, };
export type CompanyPurchaseFacts = { grants: Array<CompanyPackageGrant>, history: Array<CompanyPackageHistoryRight>, };
export type BatchChange = { expectedVersion: number, grant: CompanyPackageGrant, };
export type GrantBatch = { companyId: string, expectedRevision: string, changes: Array<BatchChange>, idempotencyKey: string, };
export type GrantBatchPreview = { companyId: string, expectedRevision: string, changes: Array<BatchChange>, before: Array<CompanyPackageGrant>, after: Array<CompanyPackageGrant>, };
export type PackageReview = { token: string, expiresAt: number, };
export type NativeServicePackagePreview = { comparison: GrantBatchPreview, request: GrantBatch, review: PackageReview, };
export type NativeServicePackageOverview = { companyId: string, revision: string, packages: Array<PackageDefinition>, billingMode: string, observedAt: number, grants: Array<CompanyPackageGrant>, history: Array<CompanyPackageHistoryRight>, };
export type NativeServiceCompany = { id: string, slug: string, name: string, holdingId: string, holding: string, };
export type NativeServiceCompanies = { companies: Array<NativeServiceCompany>, canApply: boolean, };
export type ServicePackagePreviewRequest = { holdingId: string, request: GrantBatch, };
export type ServicePackageApplyRequest = { holdingId: string, request: GrantBatch, token: string, };
