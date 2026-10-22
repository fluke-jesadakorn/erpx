import { z } from 'zod';
import { acceptanceGates, businessAcceptanceManifest, type GateEvidence } from '@erpx/catalogue/business-acceptance';

const schema = z.object({
  version: z.literal(1),
  reviews: z.array(z.object({
    capabilityId: z.string().min(1),
    gate: z.enum(acceptanceGates),
    state: z.enum(['pending', 'passed', 'not_applicable']),
    evidence: z.string().trim().min(1),
    reviewer: z.string().trim().min(1),
    reviewedAt: z.string().datetime({ offset: true }),
    reason: z.string().trim().min(3),
  }).strict()),
}).strict();

/** The reviewed source survives generation; observations never promote a gate. */
export function reviewedCapabilityManifest(input: unknown) {
  const source = schema.parse(input);
  const capabilities = businessAcceptanceManifest();
  const byId = new Map(capabilities.map(row => [row.id, row]));
  const seen = new Set<string>();
  for (const review of source.reviews) {
    const capability = byId.get(review.capabilityId);
    const key = `${review.capabilityId}:${review.gate}`;
    if (!capability || seen.has(key)) throw new Error(`Invalid or duplicate gate review: ${key}`);
    if (review.state === 'not_applicable' && review.gate !== 'provider')
      throw new Error(`Only a provider gate can be not_applicable: ${key}`);
    seen.add(key);
    const { state, evidence, reviewer, reviewedAt } = review;
    capability.gates[review.gate] = { state, evidence, reviewer, reviewedAt } satisfies GateEvidence;
  }
  return capabilities;
}

export function moduleGateSummary(capabilities: ReturnType<typeof businessAcceptanceManifest>, moduleId: string) {
  const rows = capabilities.filter(row => row.id.startsWith(`${moduleId}/`));
  return {
    capabilities: rows.length,
    ...Object.fromEntries(acceptanceGates.map(gate => [gate, rows.length > 0 && rows.every(row =>
      row.gates[gate].state === 'passed' || (gate === 'provider' && row.gates[gate].state === 'not_applicable'))])),
  };
}
