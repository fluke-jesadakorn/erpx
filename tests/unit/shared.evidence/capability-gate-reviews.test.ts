import { expect, it } from 'vitest';
import { reviewedCapabilityManifest, moduleGateSummary } from "@erpx/catalogue/capability-gate-reviews";
import { acceptanceGates, businessAcceptanceManifest } from "@erpx/catalogue/business-acceptance";

it('retains explicit reviews across generation and reads passed/not_applicable with the same vocabulary', () => {
  const capabilities = businessAcceptanceManifest().filter(row => row.id.startsWith('pos/'));
  const source = { version: 1, reviews: capabilities.flatMap(row => acceptanceGates.map(gate => ({ capabilityId: row.id, gate,
    state: gate === 'provider' ? 'not_applicable' : 'passed', evidence: 'Reviewed scoped evidence reference', reviewer: 'Independent reviewer', reviewedAt: '2026-09-24T00:00:00Z', reason: 'Manual settlement; no provider in this reviewed capability' }))) };
  const first = reviewedCapabilityManifest(source);
  expect(reviewedCapabilityManifest(source)).toEqual(first);
  expect(moduleGateSummary(first, 'pos')).toMatchObject({ automated: true, browser: true, provider: true, production: true });
  expect(moduleGateSummary(first, 'missing')).toMatchObject({ capabilities: 0, automated: false, production: false });
  expect(first.filter(row => !row.id.startsWith('pos/')).every(row => Object.values(row.gates).every(gate => gate.state === 'pending'))).toBe(true);
  const review = source.reviews[0];
  for (const changes of [{ state: 'accepted' }, { state: 'not_required' }, { state: 'not_applicable', gate: 'production' }, { capabilityId: 'missing' }])
    expect(() => reviewedCapabilityManifest({ version: 1, reviews: [{ ...review, ...changes }] })).toThrow();
  expect(() => reviewedCapabilityManifest({ version: 1, reviews: [review, review] })).toThrow();
});
