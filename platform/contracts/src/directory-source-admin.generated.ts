// Generated from native Rust Directory source-admin wire types. Do not edit.
// Regenerate: bun scripts/docs/generate-directory-source-contracts.ts
export type DirectorySourceInteger = number;
export type DirectorySourcePreviewInput = { keyId: string, expectedConnectionVersion: DirectorySourceInteger, expectedSourceRevision?: DirectorySourceInteger | null,
/**
 * Trimmed ECMAScript text, 3..1000 Unicode code points at runtime.
 */
reason: string, };
export type DirectorySourceApproveInput = { expectedVersion: DirectorySourceInteger,
/**
 * Independent review reason, normalized before receipt/audit identity.
 */
reason: string, };
export type DirectorySourceStatus = "pending" | "approved";
export type DirectorySourceReviewData = { id: string, status: DirectorySourceStatus, version: DirectorySourceInteger, sourceRevision: DirectorySourceInteger, decisionReason?: string, };
export type DirectorySourceResponse = { data: DirectorySourceReviewData, };
export type DirectorySourceError = { code: string, message: string, };
export type DirectorySourceErrorResponse = { error: DirectorySourceError, };
