import assert from 'node:assert/strict';
import test from 'node:test';
import { parseOperationsBranchParam } from '../operations/scope';
import { buildBridgeReadStatus, buildBridgeShadowEvent } from './bridge-contract';
import { boundedSearch, boundedText, isUuid, safeApiError } from './input';
import { validateChildAssignmentWindow } from './rbac-contract';

test('Folio bridge reports reconciled only with complete parity and stable evidence', () => {
  const status = buildBridgeReadStatus({
    targetConfigured: true,
    sourceAvailable: true,
    targetAvailable: true,
    observedAt: '2026-07-28T00:00:00.000Z',
    counts: {
      sourceCustomerCount: 4,
      sourceActiveCustomerCount: 3,
      targetCustomerCount: 4,
      targetActiveCustomerCount: 3,
      targetLinkedCustomerCount: 4,
      targetUnlinkedCustomerCount: 0,
    },
    identityEvidence: {
      sourceIdentityDigest: 'source-ids-4',
      targetIdentityLinkDigest: 'source-ids-4',
    },
    eventHighWatermarks: { source: '42', target: '42' },
  });
  assert.equal(status.mode, 'read_only');
  assert.equal(status.readPath, 'isolated_control_read');
  assert.equal(status.reconciliation.state, 'reconciled');
  assert.deepEqual(status.reconciliation.evidence, {
    customerCountParity: true,
    activeCountParity: true,
    linkedIdentityCountParity: true,
    noUnlinkedTargets: true,
    immutableIdentityParity: true,
    sourceIdentityDigest: 'source-ids-4',
    targetIdentityLinkDigest: 'source-ids-4',
    eventHighWatermarkParity: true,
    sourceEventHighWatermark: '42',
    targetEventHighWatermark: '42',
  });
  assert.deepEqual(status.controls, {
    cutover: 'not_approved',
    folioWrites: 'retained',
    tenantWrites: 'not_initiated',
    ownership: 'folio_retained',
  });
});

test('Folio bridge preserves source aggregate identity when building shadow events', () => {
  const event = buildBridgeShadowEvent({
    eventType: 'role_assignment_revoked',
    aggregateType: 'role_assignment',
    aggregateId: 'source-assignment-42',
    aggregateVersion: 3,
    idempotencyKey: 'role-assignment-revoked-42-3',
    payload: { source: 'folio' },
    occurredAt: '2026-07-31T00:00:00.000Z',
  });
  assert.equal(event.aggregateId, 'source-assignment-42');
  assert.equal(event.aggregateType, 'folio.role_assignment');
  assert.equal(event.eventType, 'folio.bridge.role_assignment_revoked');
  assert.equal(event.aggregateVersion, 3);
});

test('Folio bridge never labels a fallback or unlinked target as reconciled', () => {
  const fallback = buildBridgeReadStatus({
    targetConfigured: false,
    sourceAvailable: true,
    targetAvailable: true,
    counts: {
      sourceCustomerCount: 1,
      sourceActiveCustomerCount: 1,
      targetCustomerCount: 1,
      targetActiveCustomerCount: 1,
      targetLinkedCustomerCount: 1,
      targetUnlinkedCustomerCount: 0,
    },
  });
  const drift = buildBridgeReadStatus({
    targetConfigured: true,
    sourceAvailable: true,
    targetAvailable: true,
    counts: {
      sourceCustomerCount: 2,
      sourceActiveCustomerCount: 2,
      targetCustomerCount: 3,
      targetActiveCustomerCount: 2,
      targetLinkedCustomerCount: 2,
      targetUnlinkedCustomerCount: 1,
    },
    identityEvidence: {
      sourceIdentityDigest: 'source-ids-2',
      targetIdentityLinkDigest: 'source-ids-2',
    },
    eventHighWatermarks: { source: '7', target: '7' },
  });
  assert.equal(fallback.reconciliation.state, 'not_configured');
  assert.equal(fallback.readPath, 'not_configured');
  assert.equal(drift.reconciliation.state, 'drift');
});

test('Folio bridge detects active-count and immutable-link drift', () => {
  const activeDrift = buildBridgeReadStatus({
    targetConfigured: true,
    sourceAvailable: true,
    targetAvailable: true,
    counts: {
      sourceCustomerCount: 2,
      sourceActiveCustomerCount: 2,
      targetCustomerCount: 2,
      targetActiveCustomerCount: 1,
      targetLinkedCustomerCount: 2,
      targetUnlinkedCustomerCount: 0,
    },
    identityEvidence: {
      sourceIdentityDigest: 'same',
      targetIdentityLinkDigest: 'same',
    },
    eventHighWatermarks: { source: '8', target: '8' },
  });
  const identityDrift = buildBridgeReadStatus({
    targetConfigured: true,
    sourceAvailable: true,
    targetAvailable: true,
    counts: {
      sourceCustomerCount: 2,
      sourceActiveCustomerCount: 2,
      targetCustomerCount: 2,
      targetActiveCustomerCount: 2,
      targetLinkedCustomerCount: 2,
      targetUnlinkedCustomerCount: 0,
    },
    identityEvidence: {
      sourceIdentityDigest: 'source-set',
      targetIdentityLinkDigest: 'different-set',
    },
    eventHighWatermarks: { source: '8', target: '8' },
  });
  assert.equal(activeDrift.reconciliation.state, 'drift');
  assert.equal(activeDrift.reconciliation.evidence.activeCountParity, false);
  assert.equal(identityDrift.reconciliation.state, 'drift');
  assert.equal(identityDrift.reconciliation.evidence.immutableIdentityParity, false);
});

test('Folio bridge detects an event high-watermark mismatch', () => {
  const status = buildBridgeReadStatus({
    targetConfigured: true,
    sourceAvailable: true,
    targetAvailable: true,
    counts: {
      sourceCustomerCount: 2,
      sourceActiveCustomerCount: 2,
      targetCustomerCount: 2,
      targetActiveCustomerCount: 2,
      targetLinkedCustomerCount: 2,
      targetUnlinkedCustomerCount: 0,
    },
    identityEvidence: {
      sourceIdentityDigest: 'same',
      targetIdentityLinkDigest: 'same',
    },
    eventHighWatermarks: { source: '10', target: '9' },
  });
  assert.equal(status.reconciliation.state, 'drift');
  assert.equal(status.reconciliation.evidence.eventHighWatermarkParity, false);
});

test('Folio bridge fails closed when identity or event evidence is unavailable', () => {
  const missingIdentity = buildBridgeReadStatus({
    targetConfigured: true,
    sourceAvailable: true,
    targetAvailable: true,
    counts: {
      sourceCustomerCount: 1,
      sourceActiveCustomerCount: 1,
      targetCustomerCount: 1,
      targetActiveCustomerCount: 1,
      targetLinkedCustomerCount: 1,
      targetUnlinkedCustomerCount: 0,
    },
    eventHighWatermarks: { source: '9', target: '9' },
  });
  const missingEvents = buildBridgeReadStatus({
    targetConfigured: true,
    sourceAvailable: true,
    targetAvailable: true,
    counts: {
      sourceCustomerCount: 1,
      sourceActiveCustomerCount: 1,
      targetCustomerCount: 1,
      targetActiveCustomerCount: 1,
      targetLinkedCustomerCount: 1,
      targetUnlinkedCustomerCount: 0,
    },
    identityEvidence: {
      sourceIdentityDigest: 'same',
      targetIdentityLinkDigest: 'same',
    },
  });
  assert.equal(missingIdentity.reconciliation.state, 'unavailable');
  assert.equal(missingIdentity.reconciliation.evidence.immutableIdentityParity, null);
  assert.equal(missingEvents.reconciliation.state, 'unavailable');
  assert.equal(missingEvents.reconciliation.evidence.eventHighWatermarkParity, null);
});

test('Folio bridge rejects oversized or non-text payload fields without truncating them', () => {
  assert.equal(boundedText(' valid ', 10), 'valid');
  assert.equal(boundedText('12345678901', 10), null);
  assert.equal(boundedText({ value: 'not text' }, 10), null);
  assert.equal(boundedSearch('account   code'), 'account code');
  assert.equal(boundedSearch('x'.repeat(121)), undefined);
});

test('Folio bridge accepts only UUID identifiers and does not expose unknown errors', () => {
  assert.equal(isUuid('9f8c2d7a-0c1c-4a1b-8c6d-9c22f6f57f1b'), true);
  assert.equal(isUuid('not-an-id'), false);
  assert.equal(safeApiError(new Error('relation platform.secret_table does not exist'), 'operation failed'), 'operation failed');
  assert.equal(safeApiError(new Error('ticket/customer scope mismatch'), 'operation failed'), 'ticket/customer scope mismatch');
});

test('operations branch parameters fail closed instead of widening to all branches', () => {
  assert.deepEqual(parseOperationsBranchParam(undefined), { branchId: null, invalid: false });
  assert.deepEqual(parseOperationsBranchParam('12'), { branchId: 12, invalid: false });
  assert.deepEqual(parseOperationsBranchParam('abc'), { branchId: null, invalid: true });
  assert.deepEqual(parseOperationsBranchParam('0'), { branchId: null, invalid: true });
});

test('child role assignments require a currently active parent window', () => {
  const now = new Date('2026-07-30T12:00:00.000Z');
  assert.throws(
    () => validateChildAssignmentWindow({
      parentValidFrom: new Date('2026-07-30T00:00:00.000Z'),
      parentValidUntil: new Date('2026-07-30T11:59:59.000Z'),
      childValidFrom: now,
      childValidUntil: new Date('2026-07-30T12:30:00.000Z'),
      now,
    }),
    /active parent assignment/,
  );
  assert.throws(
    () => validateChildAssignmentWindow({
      parentValidFrom: new Date('2026-07-30T00:00:00.000Z'),
      parentValidUntil: new Date('2026-07-30T13:00:00.000Z'),
      childValidFrom: now,
      childValidUntil: new Date('2026-07-30T13:30:00.000Z'),
      now,
    }),
    /active parent assignment/,
  );
  assert.doesNotThrow(() => validateChildAssignmentWindow({
    parentValidFrom: new Date('2026-07-30T00:00:00.000Z'),
    parentValidUntil: new Date('2026-07-30T13:00:00.000Z'),
    childValidFrom: now,
    childValidUntil: new Date('2026-07-30T12:30:00.000Z'),
    now,
  }));
});
