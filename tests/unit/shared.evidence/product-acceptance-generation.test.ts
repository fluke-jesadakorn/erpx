import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import productAcceptanceRegister from "../../../docs/status/product-migration-acceptance.json";
import { writeProductAcceptanceOutputs } from "../../../scripts/docs/product-acceptance-output";

describe("product acceptance generation", () => {
  it("keeps generated source identity on fingerprints instead of checkout commit SHA", () => {
    expect(productAcceptanceRegister.version).toBe(3);
    expect(productAcceptanceRegister.sourceFingerprint).toMatch(/^[a-f0-9]{64}$/);
    expect(productAcceptanceRegister.verificationFingerprint).toMatch(/^[a-f0-9]{64}$/);
    expect(productAcceptanceRegister).not.toHaveProperty("candidateSha");
  });

  it("does not replace generated artifacts when validation fails", async () => {
    const directory = await mkdtemp(join(tmpdir(), "erpx-product-acceptance-"));
    try {
      const register = join(directory, "acceptance.json"), plan = join(directory, "production-plan.md");
      await writeFile(register, "previous register\n");
      await writeFile(plan, "previous plan\n");

      expect(() => writeProductAcceptanceOutputs(["INVALID_RECEIPT_EVIDENCE"], [
        { path: register, contents: "new register\n" },
        { path: plan, contents: "new plan\n" },
      ])).toThrow("INVALID_RECEIPT_EVIDENCE");

      await expect(readFile(register, "utf8")).resolves.toBe("previous register\n");
      await expect(readFile(plan, "utf8")).resolves.toBe("previous plan\n");
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  });
});
