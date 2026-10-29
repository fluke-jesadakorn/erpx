// Generated from Rust Field attachment contracts. Do not edit by hand.
export type FieldServiceRenderedContext = { userId: string, holdingId: string, companyId: string, branchId: string | null, departmentId: string | null, branchSlug: string | null, departmentSlug: string | null, assignmentId: string, assignmentVersion: number, revision: number, role: string, assignmentScopeKind: string, modules: Array<string>, permissions: Array<string>, expiresAt: number, };
export type FieldAttachmentWithdrawInput = { reason: string, };
export type FieldAttachmentScan = { attempts: number, lastResult: string | null, lastError: string | null, signature: string | null, definitionUpdatedAt: number | null, scannedAt: number | null, };
export type FieldAttachmentStatus = { linkId: string, setId: string, sourceId: string | null, sourceState: string | null, linkState: string, scan: FieldAttachmentScan, previewAvailable: boolean, bindable: boolean, expiresAt: number, boundAt: number | null, withdrawnAt: number | null, };
export type FieldAttachmentStatusResponse = { data: FieldAttachmentStatus, context: FieldServiceRenderedContext, };
