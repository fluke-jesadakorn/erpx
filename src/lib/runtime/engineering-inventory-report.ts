import { z } from "zod";
import { engineeringGateIds } from "@/lib/runtime/engineering-completion";

const digest = z.string().regex(/^[a-f0-9]{64}$/);
const result = z.enum(["passed", "failed", "skipped", "not_run"]);
const schema = z.object({
  version: z.literal(1),
  scope: z.literal("catalogue-engineering-completion-v1"),
  assessedAt: z.string().datetime({ offset: true }),
  environment: z.string().min(1),
  sourceFingerprint: digest,
  verificationFingerprint: digest,
  complete: z.boolean(),
  moduleCount: z.number().int().positive(),
  capabilityCount: z.number().int().positive(),
  passedModules: z.number().int().nonnegative(),
  passedCapabilities: z.number().int().nonnegative(),
  issues: z.array(z.string()),
  capabilities: z.array(z.object({
    id: z.string().min(1), moduleId: z.string().min(1), state: result, errors: z.array(z.string()),
  }).strict()),
  gates: z.array(z.object({ id: z.enum(engineeringGateIds), state: result }).strict()),
  provenance: z.object({
    runId: z.string().min(1),
    completionSha256: digest, compiledCandidateSha256: digest,
    registrySha256: digest, scenarioRegisterSha256: digest,
    integrationResultsSha256: digest, desktopResultsSha256: digest, mobileResultsSha256: digest,
    candidateId: z.string().min(1), candidateSoftwareVersion: z.string().min(1), candidateFileManifestSha256: digest,
  }).strict(),
}).strict();

type Module = { id: string; sections: Array<{ id: string; capability: boolean }> };

/** A dated engineering result never changes provider reviews or Company release state. */
export function validateEngineeringInventory(input: unknown, registry: Module[], registrySha256: string, scenarioRegisterSha256: string) {
  const receipt = schema.parse(input);
  const expected = new Set(registry.flatMap(module => module.sections.filter(section => section.capability)
    .map(section => `${module.id}/${section.id}`)));
  const ids = receipt.capabilities.map(row => row.id);
  if (new Set(ids).size !== ids.length || ids.length !== expected.size || ids.some(id => !expected.has(id)))
    throw new Error("Engineering receipt has missing, unknown or duplicate capabilities.");
  if (receipt.capabilities.some(row => row.moduleId !== row.id.split("/")[0] || (row.state === "passed" && row.errors.length)))
    throw new Error("Engineering receipt has inconsistent capability results.");
  const gateIds = receipt.gates.map(gate => gate.id);
  if (new Set(gateIds).size !== gateIds.length || gateIds.length !== engineeringGateIds.length)
    throw new Error("Engineering receipt has missing or duplicate gates.");
  if (receipt.provenance.registrySha256 !== registrySha256 || receipt.provenance.scenarioRegisterSha256 !== scenarioRegisterSha256)
    throw new Error("Engineering receipt does not match the catalogue/scenario register.");
  if (receipt.provenance.candidateSoftwareVersion !== `release-${receipt.sourceFingerprint}`)
    throw new Error("Engineering receipt candidate does not match its recorded source.");
  const passedCapabilities = receipt.capabilities.filter(row => row.state === "passed").length;
  const passedModules = registry.filter(module => receipt.capabilities.filter(row => row.moduleId === module.id)
    .every(row => row.state === "passed")).length;
  const complete = receipt.issues.length === 0 && passedCapabilities === expected.size && receipt.gates.every(gate => gate.state === "passed");
  if (receipt.moduleCount !== registry.length || receipt.capabilityCount !== expected.size ||
      receipt.passedCapabilities !== passedCapabilities || receipt.passedModules !== passedModules || receipt.complete !== complete)
    throw new Error("Engineering receipt aggregate claims disagree with individual results.");
  return receipt;
}

export function inventoryVerification(row: { status: string }, engineering: { state: string }) {
  return {
    engineering: engineering.state,
    provider: row.status === "sandbox_verified" ? "Sandbox evidence; named billing journey only"
      : row.status === "live_verified" ? "Isolated real-service lab evidence; not production activation"
        : "No provider verification recorded in this inventory",
    companyRelease: "Separate Company/build/configuration review; not granted by this inventory",
  };
}
