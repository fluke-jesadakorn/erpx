// Generated from the Rust Field attachment intake contracts. Do not edit.
export type FieldIntakeTarget = "field_service_visit";

export type FieldIntakeComponent = "record" | "action" | "line" | "event" | "message" | "publication";

export type FieldIntakeState = "draft" | "bound" | "withdrawn" | "expired";

export type FieldIntakeClassification = "public" | "internal" | "confidential" | "restricted";

export type FieldIntakeSourceState = "legacy_unscanned" | "quarantined" | "scanning" | "available" | "scan_error" | "malware_rejected" | "destroyed";

export type FieldIntakeCreateInput = { targetType: FieldIntakeTarget, targetId: string, componentKind: FieldIntakeComponent, componentRef?: string, purposeKey: string, expectedTargetVersion: number, };

export type FieldIntakeLink = { id: string, sourceId: string | null, filename: string, mediaType: string, bytes: number | null, sourceState: FieldIntakeSourceState | null, linkState: FieldIntakeState, caption: string | null, previewAvailable: boolean, createdAt: number, expiresAt: number, boundAt: number | null, };

export type FieldIntakeSet = { id: string, targetType: FieldIntakeTarget, targetId: string, componentKind: FieldIntakeComponent, componentRef: string | null, purposeKey: string, expectedTargetVersion: number, policyVersion: number, classification: FieldIntakeClassification, requiredCount: number, maxFiles: number, state: FieldIntakeState, createdAt: number, updatedAt: number, links: Array<FieldIntakeLink>, };

export type FieldIntakeListResponse = { data: Array<FieldIntakeSet>, };

export type FieldIntakeCreateResponse = { data: FieldIntakeSet, };

export type FieldFileReserveInput = { filename: string, mediaType: string, bytes: number, caption?: string | null, supersedesLinkId?: string | null, };

export type FieldFileReservationData = { uploadIntentId: string, attachmentLinkId: string, expiresAt: number, uploadUrl: string, };

export type FieldFileReservationResponse = { data: FieldFileReservationData, };

export type FieldFileUploadData = { uploadIntentId: string, attachmentLinkId: string, sourceId: string, state: FieldIntakeSourceState, status: string, };

export type FieldFileUploadResponse = { data: FieldFileUploadData, };

export type FieldFileScanStatus = { attempts: number, lastResult: string | null, lastError: string | null, signature: string | null, definitionUpdatedAt: number | null, scannedAt: number | null, };

export type FieldFileStatusData = { linkId: string, setId: string, sourceId: string | null, sourceState: FieldIntakeSourceState | null, linkState: FieldIntakeState, scan: FieldFileScanStatus, previewAvailable: boolean, bindable: boolean, expiresAt: number, boundAt: number | null, withdrawnAt: number | null, };

export type FieldFileStatusResponse = { data: FieldFileStatusData, };

export type FieldIntakeError = { code: string, message: string, };

export type FieldIntakeErrorResponse = { error: FieldIntakeError, };
