import { z } from "zod";
import { acceptanceGates, businessAcceptanceManifest } from "@erpx/catalogue/business-acceptance";

const text = z.string().trim().min(1);
const schema = z.object({
  version: z.literal(1),
  records: z.array(z.object({
    id: text,
    capabilityIds: z.array(text).min(1),
    scenarioIds: z.array(z.string().regex(/^S\d{2}$/)).min(1),
    gate: z.enum(acceptanceGates),
    environment: z.enum(["simulation", "sandbox", "production"]),
    outcome: z.enum(["passed", "failed", "partial", "blocked"]),
    scope: text,
    observedAt: z.string().datetime({ offset: true }),
    recordedBy: text,
    evidence: z.array(z.object({
      path: text,
      sha256: z.string().regex(/^[a-f0-9]{64}$/),
    }).strict()).min(1),
    limitations: z.array(text),
    supersedes: text.nullable(),
  }).strict()),
}).strict();

export type AcceptanceEvidenceLedger = z.infer<typeof schema>;

/** Observations are not capability sign-offs. Keep gate state independent. */
export function validateAcceptanceEvidence(input: unknown): AcceptanceEvidenceLedger {
  const ledger = schema.parse(input);
  const capabilities = new Map(businessAcceptanceManifest().map(row => [row.id, row]));
  const seen = new Map<string, AcceptanceEvidenceLedger["records"][number]>();
  for (const record of ledger.records) {
    if (seen.has(record.id)) throw new Error(`Duplicate evidence ID: ${record.id}`);
    if (new Set(record.capabilityIds).size !== record.capabilityIds.length ||
        new Set(record.scenarioIds).size !== record.scenarioIds.length)
      throw new Error(`Duplicate coverage: ${record.id}`);
    const linkedScenarios = new Set<string>();
    for (const capabilityId of record.capabilityIds) {
      const capability = capabilities.get(capabilityId);
      if (!capability) throw new Error(`Unknown capability: ${capabilityId}`);
      for (const scenario of capability.scenarios) linkedScenarios.add(scenario);
    }
    if (record.scenarioIds.some(id => !linkedScenarios.has(id)))
      throw new Error(`Scenario outside capability mapping: ${record.id}`);
    if (record.gate === "production" && record.environment !== "production")
      throw new Error(`Simulation cannot attest production: ${record.id}`);
    if (record.gate === "provider" && record.environment === "simulation")
      throw new Error(`Mocks cannot attest provider acceptance: ${record.id}`);
    for (const evidence of record.evidence) {
      if (!evidence.path.startsWith("docs/testing/evidence/") ||
          evidence.path.includes("\\") || evidence.path.split("/").some(part => part === ".." || part === "." || !part))
        throw new Error(`Evidence must use durable repository paths: ${record.id}`);
    }
    if (record.supersedes) {
      const previous = seen.get(record.supersedes);
      if (!previous || previous.gate !== record.gate || previous.environment !== record.environment ||
          [...previous.capabilityIds].sort().join() !== [...record.capabilityIds].sort().join() ||
          [...previous.scenarioIds].sort().join() !== [...record.scenarioIds].sort().join() ||
          Date.parse(previous.observedAt) > Date.parse(record.observedAt))
        throw new Error(`Invalid evidence supersession: ${record.id}`);
    }
    seen.set(record.id, record);
  }
  return ledger;
}
