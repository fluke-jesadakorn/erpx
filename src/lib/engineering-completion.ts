import { z } from "zod";

const digest = z.string().regex(/^[a-f0-9]{64}$/);
const relativeTestFile = z.string().regex(/^tests\/(?!.*(?:^|\/)\.\.\/)[\w./-]+\.(?:test\.(?:ts|tsx)|spec\.ts)$/);
export const engineeringCaseSchema = z.object({
  runner: z.enum(["vitest", "playwright"]), file: relativeTestFile,
  title: z.string().trim().min(1), proof: z.string().trim().min(20),
  project: z.enum(["desktop", "mobile"]).optional(),
}).superRefine((item, context) => {
  if (item.runner === "playwright" && !item.project)
    context.addIssue({ code: "custom", message: "A required browser case must identify its desktop/mobile project.", path: ["project"] });
  if (item.runner === "vitest" && item.project)
    context.addIssue({ code: "custom", message: "Integration cases have no browser project.", path: ["project"] });
});
export const engineeringManifestSchema = z.object({
  version: z.literal(1), scope: z.literal("catalogue-engineering-completion-v1"), registrySha256: digest,
  modules: z.array(z.string().min(1)).min(1),
  capabilities: z.array(z.object({
    id: z.string().min(1), moduleId: z.string().min(1), capabilityId: z.string().min(1),
    workflow: z.array(z.string().trim().min(1)).min(1), expectedResults: z.array(z.string().trim().min(1)).min(1),
    review: z.enum(["pending", "reviewed"]), cases: z.array(engineeringCaseSchema),
  })).min(1),
});
export type EngineeringManifest = z.infer<typeof engineeringManifestSchema>;
export type EngineeringObservation = {
  runner: "vitest" | "playwright"; file: string; title: string;
  status: "passed" | "failed" | "skipped" | "not_run"; durationMs?: number; artifacts?: string[];
  project?: "desktop" | "mobile";
};
export const engineeringGateIds = ["typecheck", "lint", "integration", "browser", "release-build", "migration-upgrade",
  "recovery", "attachments", "legal-uat", "financial-continuity", "calendar-delivery", "performance"] as const;
export type EngineeringGateId = typeof engineeringGateIds[number];
export type EngineeringGate = {
  id: EngineeringGateId; status: "passed" | "failed" | "not_run";
  sourceFingerprint: string; verificationFingerprint: string; artifact: string; sha256: string;
};
type RegistryModule = { id: string; sections: Array<{ id: string; capability: boolean }> };
const caseKey = (item: Pick<EngineeringObservation, "runner" | "file" | "title" | "project">) => JSON.stringify([item.runner, item.file, item.title, item.project ?? ""]);

/** No test-file presence, old acceptance review or HTTP status can satisfy this gate. */
export function evaluateEngineeringCompletion(input: {
  manifest: EngineeringManifest; registry: RegistryModule[]; registrySha256: string;
  sourceFingerprint: string; verificationFingerprint: string;
  observations: EngineeringObservation[]; gates: EngineeringGate[];
}) {
  const issues: string[] = [], expectedModules = input.registry.map(module => module.id);
  const expectedCapabilities = input.registry.flatMap(module => module.sections.filter(section => section.capability)
    .map(section => `${module.id}/${section.id}`));
  const manifest = engineeringManifestSchema.parse(input.manifest);
  if (manifest.registrySha256 !== input.registrySha256) issues.push("Catalogue bytes changed; review the scenario register.");
  for (const [label, actual, expected] of [["module", manifest.modules, expectedModules],
    ["capability", manifest.capabilities.map(item => item.id), expectedCapabilities]] as const) {
    if (new Set(actual).size !== actual.length) issues.push(`Duplicate ${label} in scenario register.`);
    for (const id of expected) if (!actual.includes(id)) issues.push(`Missing ${label}: ${id}`);
    for (const id of actual) if (!expected.includes(id)) issues.push(`Unknown ${label}: ${id}`);
  }
  const observations = new Map<string, EngineeringObservation[]>();
  for (const observation of input.observations) {
    const key = caseKey(observation);
    observations.set(key, [...(observations.get(key) ?? []), observation]);
  }
  const capabilities = manifest.capabilities.map(capability => {
    const errors: string[] = [];
    if (capability.id !== `${capability.moduleId}/${capability.capabilityId}`) errors.push("Capability identity mismatch.");
    if (capability.review !== "reviewed") errors.push("Workflow assertions await engineering review.");
    if (!capability.cases.length) errors.push("No required scenario maps to this capability.");
    if (new Set(capability.cases.map(caseKey)).size !== capability.cases.length) errors.push("Duplicate required scenario.");
    for (const required of capability.cases) {
      const matches = observations.get(caseKey(required)) ?? [];
      if (!matches.length) errors.push(`Not run: ${required.file} :: ${required.title}`);
      else if (matches.length !== 1) errors.push(`Ambiguous/repeated receipt: ${required.file} :: ${required.title}`);
      else if (matches[0].status !== "passed") errors.push(`${matches[0].status}: ${required.file} :: ${required.title}`);
    }
    return { id: capability.id, moduleId: capability.moduleId, passed: errors.length === 0, errors };
  });
  const gates = engineeringGateIds.map(id => {
    const matches = input.gates.filter(gate => gate.id === id), gate = matches[0];
    const passed = matches.length === 1 && gate.status === "passed" && gate.sourceFingerprint === input.sourceFingerprint &&
      gate.verificationFingerprint === input.verificationFingerprint && /^[a-f0-9]{64}$/.test(gate.sha256) && Boolean(gate.artifact);
    if (!passed) issues.push(`Required gate ${id}: ${!matches.length ? "not run" : matches.length > 1 ? "duplicate receipt" : gate.status !== "passed" ? gate.status : "source/evidence mismatch"}`);
    return { id, passed };
  });
  const completeModules = expectedModules.filter(id => {
    const required = expectedCapabilities.filter(key => key.startsWith(`${id}/`));
    return required.length > 0 && required.every(key => capabilities.find(item => item.id === key)?.passed);
  });
  return {
    complete: issues.length === 0 && capabilities.length === expectedCapabilities.length && capabilities.every(item => item.passed) && gates.every(gate => gate.passed),
    moduleCount: expectedModules.length, capabilityCount: expectedCapabilities.length,
    passedModules: completeModules.length, passedCapabilities: capabilities.filter(item => item.passed).length,
    issues, capabilities, gates,
  };
}
