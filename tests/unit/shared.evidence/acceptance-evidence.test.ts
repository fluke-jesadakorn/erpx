import { describe, expect, it } from "vitest";
import ledger from "../../../docs/status/business-acceptance-evidence.json";
import { validateAcceptanceEvidence } from "@erpx/catalogue/acceptance-evidence";

describe("acceptance evidence boundaries", () => {
  const record = ledger.records[0];
  const withRecord = (changes: Record<string, unknown>) => ({ version: 1, records: [{ ...record, ...changes }] });

  it("rejects simulation evidence for provider and production gates", () => {
    for (const gate of ["provider", "production"]) expect(() => validateAcceptanceEvidence(withRecord({ gate }))).toThrow();
  });
  it("rejects stale capability/scenario mappings and non-durable evidence", () => {
    for (const changes of [
      { capabilityIds: ["finance/missing"] }, { scenarioIds: ["S99"] },
      { evidence: [{ ...record.evidence[0], path: ".scratch/result.log" }] },
      { evidence: [{ ...record.evidence[0], path: "docs/testing/evidence/../secret" }] },
    ]) expect(() => validateAcceptanceEvidence(withRecord(changes))).toThrow();
  });
  it("requires unique IDs and an earlier, same-scope record for corrections", () => {
    expect(() => validateAcceptanceEvidence({ version: 1, records: [record, record] })).toThrow();
    expect(() => validateAcceptanceEvidence(withRecord({ supersedes: "missing" }))).toThrow();
    const correction = { ...record, id: "correction", supersedes: record.id };
    expect(validateAcceptanceEvidence({ version: 1, records: [record, correction] }).records).toHaveLength(2);
    expect(() => validateAcceptanceEvidence({ version: 1, records: [record, { ...correction, gate: "browser" }] })).toThrow();
  });
});
