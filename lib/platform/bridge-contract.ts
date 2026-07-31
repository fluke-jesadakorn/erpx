export type BridgeReconciliationState = 'reconciled' | 'drift' | 'unavailable' | 'not_configured';

export interface BridgeCustomerCounts {
  sourceCustomerCount: number | null;
  sourceActiveCustomerCount: number | null;
  targetCustomerCount: number | null;
  targetActiveCustomerCount: number | null;
  targetLinkedCustomerCount: number | null;
  targetUnlinkedCustomerCount: number | null;
}

/**
 * A stable identity evidence pair. The source digest is built from the
 * immutable Folio customer IDs; the target digest is built from the target's
 * immutable legacy-customer links. The two digests must match before the
 * bridge can claim that the populations are the same.
 */
export interface BridgeIdentityEvidence {
  sourceIdentityDigest: string | null;
  targetIdentityLinkDigest: string | null;
}

/**
 * Event watermarks are strings because PostgreSQL sequence IDs may exceed the
 * JavaScript safe-integer range. They are opaque but must be present on both
 * sides and equal for a reconciled result.
 */
export interface BridgeEventHighWatermarks {
  source: string | null;
  target: string | null;
}

export interface BridgeShadowEvent {
  eventType: string;
  aggregateType: string;
  aggregateId: string;
  aggregateVersion: number;
  idempotencyKey: string;
  payload: Record<string, unknown>;
  occurredAt: string;
}

/** Build the target event identity without conflating the queue row with its source aggregate. */
export function buildBridgeShadowEvent(input: {
  eventType: string;
  aggregateType: string;
  aggregateId: string;
  aggregateVersion: number;
  idempotencyKey: string;
  payload: Record<string, unknown>;
  occurredAt: string;
}): BridgeShadowEvent {
  return {
    eventType: `folio.bridge.${input.eventType}`,
    aggregateType: `folio.${input.aggregateType}`,
    aggregateId: input.aggregateId,
    aggregateVersion: input.aggregateVersion,
    idempotencyKey: input.idempotencyKey,
    payload: input.payload,
    occurredAt: input.occurredAt,
  };
}

export interface BridgeReconciliationEvidence {
  customerCountParity: boolean | null;
  activeCountParity: boolean | null;
  linkedIdentityCountParity: boolean | null;
  noUnlinkedTargets: boolean | null;
  immutableIdentityParity: boolean | null;
  sourceIdentityDigest: string | null;
  targetIdentityLinkDigest: string | null;
  eventHighWatermarkParity: boolean | null;
  sourceEventHighWatermark: string | null;
  targetEventHighWatermark: string | null;
}

export interface FolioBridgeReadStatus {
  integration: 'folio-erpx-bridge';
  mode: 'read_only';
  source: 'folio_operational_read_model';
  target: 'erpx_control_read_model';
  targetConfigured: boolean;
  readPath: 'isolated_control_read' | 'not_configured';
  reconciliation: {
    state: BridgeReconciliationState;
    observedAt: string;
    counts: BridgeCustomerCounts;
    evidence: BridgeReconciliationEvidence;
  };
  controls: {
    cutover: 'not_approved';
    folioWrites: 'retained';
    tenantWrites: 'not_initiated';
    ownership: 'folio_retained';
  };
}

export function buildBridgeReadStatus(args: {
  targetConfigured: boolean;
  sourceAvailable: boolean;
  targetAvailable: boolean;
  counts: BridgeCustomerCounts;
  identityEvidence?: BridgeIdentityEvidence;
  eventHighWatermarks?: BridgeEventHighWatermarks;
  observedAt?: string;
}): FolioBridgeReadStatus {
  const sourceIdentityDigest = presentEvidence(args.identityEvidence?.sourceIdentityDigest);
  const targetIdentityLinkDigest = presentEvidence(args.identityEvidence?.targetIdentityLinkDigest);
  const sourceEventHighWatermark = presentEvidence(args.eventHighWatermarks?.source);
  const targetEventHighWatermark = presentEvidence(args.eventHighWatermarks?.target);

  const customerCountParity = comparableNonNegativeCounts(
    args.counts.sourceCustomerCount,
    args.counts.targetCustomerCount,
  )
    ? args.counts.sourceCustomerCount === args.counts.targetCustomerCount
    : null;
  const activeCountParity = comparableNonNegativeCounts(
    args.counts.sourceActiveCustomerCount,
    args.counts.targetActiveCustomerCount,
  )
    ? args.counts.sourceActiveCustomerCount === args.counts.targetActiveCustomerCount
    : null;
  const linkedIdentityCountParity = comparableNonNegativeCounts(
    args.counts.sourceCustomerCount,
    args.counts.targetLinkedCustomerCount,
  )
    ? args.counts.sourceCustomerCount === args.counts.targetLinkedCustomerCount
    : null;
  const noUnlinkedTargets = isNonNegativeCount(args.counts.targetUnlinkedCustomerCount)
    ? args.counts.targetUnlinkedCustomerCount === 0
    : null;
  const immutableIdentityParity = sourceIdentityDigest !== null && targetIdentityLinkDigest !== null
    ? sourceIdentityDigest === targetIdentityLinkDigest
    : null;
  const eventHighWatermarkParity = sourceEventHighWatermark !== null && targetEventHighWatermark !== null
    ? sourceEventHighWatermark === targetEventHighWatermark
    : null;

  const evidence: BridgeReconciliationEvidence = {
    customerCountParity,
    activeCountParity,
    linkedIdentityCountParity,
    noUnlinkedTargets,
    immutableIdentityParity,
    sourceIdentityDigest,
    targetIdentityLinkDigest,
    eventHighWatermarkParity,
    sourceEventHighWatermark,
    targetEventHighWatermark,
  };

  const state: BridgeReconciliationState = !args.sourceAvailable || !args.targetAvailable
    ? 'unavailable'
    : !args.targetConfigured
      ? 'not_configured'
      : !allEvidenceAvailable(evidence)
        ? 'unavailable'
        : allEvidenceMatches(evidence)
          ? 'reconciled'
          : 'drift';

  return {
    integration: 'folio-erpx-bridge',
    mode: 'read_only',
    source: 'folio_operational_read_model',
    target: 'erpx_control_read_model',
    targetConfigured: args.targetConfigured,
    readPath: args.targetConfigured ? 'isolated_control_read' : 'not_configured',
    reconciliation: {
      state,
      observedAt: args.observedAt ?? new Date().toISOString(),
      counts: args.counts,
      evidence,
    },
    controls: {
      cutover: 'not_approved',
      folioWrites: 'retained',
      tenantWrites: 'not_initiated',
      ownership: 'folio_retained',
    },
  };
}

function presentEvidence(value: string | null | undefined): string | null {
  if (typeof value !== 'string') return null;
  const normalized = value.trim();
  return normalized.length > 0 ? normalized : null;
}

function isNonNegativeCount(value: number | null): value is number {
  return value !== null && Number.isSafeInteger(value) && value >= 0;
}

function comparableNonNegativeCounts(left: number | null, right: number | null): boolean {
  return isNonNegativeCount(left) && isNonNegativeCount(right);
}

function allEvidenceAvailable(evidence: BridgeReconciliationEvidence): boolean {
  return evidence.customerCountParity !== null
    && evidence.activeCountParity !== null
    && evidence.linkedIdentityCountParity !== null
    && evidence.noUnlinkedTargets !== null
    && evidence.immutableIdentityParity !== null
    && evidence.eventHighWatermarkParity !== null;
}

function allEvidenceMatches(evidence: BridgeReconciliationEvidence): boolean {
  return evidence.customerCountParity === true
    && evidence.activeCountParity === true
    && evidence.linkedIdentityCountParity === true
    && evidence.noUnlinkedTargets === true
    && evidence.immutableIdentityParity === true
    && evidence.eventHighWatermarkParity === true;
}
