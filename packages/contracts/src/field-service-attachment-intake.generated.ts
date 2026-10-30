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

export type FieldIntakeError = { code: string, message: string, };

export type FieldIntakeErrorResponse = { error: FieldIntakeError, };
