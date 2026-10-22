import { describe, expect, it } from "vitest";
import { engineeringGateIds, evaluateEngineeringCompletion, type EngineeringGate, type EngineeringManifest, type EngineeringObservation } from "@erpx/feature-runtime/lib/engineering-completion";
import {requiredBrowserExecutionsValid} from "../../../scripts/testing/engineering/engineering-test-results";

const source = "a".repeat(64), verification = "b".repeat(64), catalogue = "c".repeat(64);
const registry = [{ id: "finance", sections: [{ id: "close", capability: true }] }, { id: "people", sections: [{ id: "payroll", capability: true }] }];
const required = { runner: "vitest" as const, file: "tests/ledger-payroll.test.ts", title: "posts payroll and closes its period",
  proof: "Asserts independently approved net-pay liabilities, the balanced GL and immutable closed-period evidence." };
const manifest = (): EngineeringManifest => ({ version: 1, scope: "catalogue-engineering-completion-v1", registrySha256: catalogue, modules: ["finance", "people"],
  capabilities: registry.map(module => ({ id: `${module.id}/${module.sections[0].id}`, moduleId: module.id, capabilityId: module.sections[0].id,
    workflow: ["Prepare, independently review, post, close and reload"], expectedResults: ["Net liability and GL reconcile after reload"], review: "reviewed", cases: [required] })) });
const observations = (): EngineeringObservation[] => [{ ...required, status: "passed" }];
const gates = (): EngineeringGate[] => engineeringGateIds.map(id => ({ id, status: "passed", sourceFingerprint: source,
  verificationFingerprint: verification, artifact: `${id}.json`, sha256: "d".repeat(64) }));
function evaluate(changes: Partial<Parameters<typeof evaluateEngineeringCompletion>[0]> = {}) {
  return evaluateEngineeringCompletion({ manifest: manifest(), registry, registrySha256: catalogue, sourceFingerprint: source,
    verificationFingerprint: verification, observations: observations(), gates: gates(), ...changes });
}
describe("fail-closed engineering completion", () => {
  it('requires every collected shared journey even when the capability subset already passes',()=>{
    const required:EngineeringObservation[]=[{runner:'playwright',file:'tests/e2e/demo-core.spec.ts',title:'native form hydration',project:'desktop',status:'skipped'},{runner:'playwright',file:'tests/e2e/demo-core.spec.ts',title:'native form hydration',project:'mobile',status:'skipped'},{runner:'playwright',file:'tests/e2e/demo-shared.spec.ts',title:'unmapped shared source reload',project:'desktop',status:'skipped'}],executed=required.map(item=>({...item,status:'passed' as const}));
    expect(requiredBrowserExecutionsValid(required,executed)).toBe(true);
    expect(requiredBrowserExecutionsValid(required,executed.slice(0,2))).toBe(false);
    expect(requiredBrowserExecutionsValid(required,[executed[0],executed[1],executed[0]])).toBe(false);
    expect(requiredBrowserExecutionsValid(required,[...executed.slice(0,2),{...executed[2],status:'skipped'}])).toBe(false);
    expect(requiredBrowserExecutionsValid(required,[...executed.slice(0,2),{...executed[2],title:'another journey'}])).toBe(false);
    expect(requiredBrowserExecutionsValid([],[])).toBe(false);
  });
  it("allows reviewed cross-module assertions to prove both capabilities on one frozen release", () => {
    expect(evaluate()).toMatchObject({ complete: true, passedModules: 2, passedCapabilities: 2 });
  });
  it("fails if a catalogue scenario is omitted, unreviewed or has no required assertions", () => {
    const missing = manifest(); missing.capabilities.pop();
    expect(evaluate({ manifest: missing }).complete).toBe(false);
    const unreviewed = manifest(); unreviewed.capabilities[0].review = "pending";
    expect(evaluate({ manifest: unreviewed }).passedModules).toBe(1);
    const empty = manifest(); empty.capabilities[0].cases = [];
    expect(evaluate({ manifest: empty }).complete).toBe(false);
  });
  it("fails required skipped, not-run and duplicate receipts even when aggregate suite totals are green", () => {
    for (const status of ["skipped", "not_run", "failed"] as const)
      expect(evaluate({ observations: [{ ...required, status }] }).complete).toBe(false);
    expect(evaluate({ observations: [] }).complete).toBe(false);
    expect(evaluate({ observations: [...observations(), ...observations()] }).complete).toBe(false);
  });
  it("fails stale catalogue, runtime or assertion fingerprints and absent recovery/performance evidence", () => {
    expect(evaluate({ registrySha256: "f".repeat(64) }).complete).toBe(false);
    expect(evaluate({ sourceFingerprint: "f".repeat(64) }).complete).toBe(false);
    expect(evaluate({ verificationFingerprint: "f".repeat(64) }).complete).toBe(false);
    expect(evaluate({ gates: gates().filter(gate => gate.id !== "recovery") }).complete).toBe(false);
    expect(evaluate({ gates: gates().filter(gate => gate.id !== "performance") }).complete).toBe(false);
  });
  it("requires independently executed desktop and mobile browser cases", () => {
    const m = manifest(), base = { runner: "playwright" as const, file: "tests/e2e/people_payroll/demo-payroll-runs.spec.ts", title: "Payroll approval survives reload", proof: "Normal-login preparer and independent reviewer assert persisted net totals and versioned decisions." };
    m.capabilities[0].cases = [{ ...base, project: "desktop" }, { ...base, project: "mobile" }];
    expect(evaluate({ manifest: m, observations: [...observations(), { ...base, project: "desktop", status: "passed" }] }).complete).toBe(false);
    expect(evaluate({ manifest: m, observations: [...observations(), { ...base, project: "desktop", status: "passed" }, { ...base, project: "mobile", status: "passed" }] }).complete).toBe(true);
  });
});
