import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import registry from "../../../docs/archive/2026-10-03-engineering-inputs/module-registry.json";
import receipt from "../../../docs/testing/evidence/2026-10-03-engineering-completion.json";
import reviews from "../../../docs/status/capability-gate-reviews.json";
import { validateEngineeringInventory, inventoryVerification } from "../../../src/lib/runtime/engineering-inventory-report";
import { reviewedCapabilityManifest } from "../../../src/data/capability-gate-reviews";

const hash = (path: string) => createHash("sha256").update(readFileSync(path)).digest("hex");
const validate = (value: unknown) => validateEngineeringInventory(value, registry,
  hash("docs/archive/2026-10-03-engineering-inputs/module-registry.json"), hash("docs/archive/2026-10-03-engineering-inputs/engineering-scenarios.json"));

describe("dated engineering inventory", () => {
  it("requires exact capability coverage and does not accept aggregate counts alone", () => {
    expect(validate(receipt).passedCapabilities).toBe(187);
    const missing = structuredClone(receipt);
    missing.capabilities.pop();
    expect(() => validate(missing)).toThrow(/missing, unknown or duplicate/);
    const duplicate = structuredClone(receipt);
    duplicate.capabilities[1] = duplicate.capabilities[0];
    expect(() => validate(duplicate)).toThrow(/missing, unknown or duplicate/);
    expect(() => validate({ ...receipt, passedCapabilities: 186 })).toThrow(/aggregate claims/);
  });

  it("rejects duplicate gates, changed scenario bytes and a mismatched candidate", () => {
    const duplicate = structuredClone(receipt);
    duplicate.gates[1] = duplicate.gates[0];
    expect(() => validate(duplicate)).toThrow(/duplicate gates/);
    expect(() => validate({ ...receipt, provenance: { ...receipt.provenance, scenarioRegisterSha256: "a".repeat(64) } }))
      .toThrow(/scenario register/);
    expect(() => validate({ ...receipt, provenance: { ...receipt.provenance, candidateSoftwareVersion: "release-wrong" } }))
      .toThrow(/recorded source/);
  });

  it.each(["failed", "skipped", "not_run"])("reports %s without treating it as accepted", state => {
    const incomplete = structuredClone(receipt) as unknown as {
      capabilities: Array<{ id: string; moduleId: string; state: string; errors: string[] }>;
      complete: boolean; passedCapabilities: number; passedModules: number;
    };
    incomplete.capabilities[0].state = state;
    incomplete.capabilities[0].errors = [`Required case ${state}`];
    incomplete.complete = false;
    incomplete.passedCapabilities--;
    incomplete.passedModules--;
    const validated = validate(incomplete);
    expect(inventoryVerification({ status: "flow_accepted" }, validated.capabilities[0]).engineering).toBe(state);
    expect(validated.complete).toBe(false);
  });

  it.each(["failed", "not_run"] as const)("retains a %s required gate despite complete capability coverage", state => {
    const incomplete = structuredClone(receipt);
    incomplete.gates[0].state = state;
    incomplete.complete = false;
    const validated = validate(incomplete);
    expect(validated.passedCapabilities).toBe(187);
    expect(validated.gates[0].state).toBe(state);
    expect(validated.complete).toBe(false);
    expect(() => validate({ ...incomplete, complete: true })).toThrow(/aggregate claims/);
  });

  it("does not infer provider evidence or promote explicit reviews from an engineering pass", () => {
    const before = reviewedCapabilityManifest(reviews);
    const validated = validate(receipt);
    expect(inventoryVerification({ status: "flow_accepted" }, validated.capabilities[0]).provider)
      .toBe("No provider verification recorded in this inventory");
    expect(inventoryVerification({ status: "live_verified" }, validated.capabilities[0]).provider)
      .toContain("Isolated real-service lab");
    expect(inventoryVerification({ status: "sandbox_verified" }, validated.capabilities[0]).companyRelease)
      .toContain("not granted");
    expect(reviewedCapabilityManifest(reviews)).toEqual(before);
    expect(before.every(row => Object.values(row.gates).every(gate => gate.state === "pending"))).toBe(true);
  });
});
