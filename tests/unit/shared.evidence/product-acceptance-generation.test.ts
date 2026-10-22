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

  it("maps active employee-expense, asset, payment and Sales invoice sources to their workflows", () => {
    const workflow = (moduleId: string, id: string) => {
      const owner = productAcceptanceRegister.modules.find(module => module.moduleId === moduleId);
      expect(owner, `Generated ${moduleId} module`).toBeDefined();
      const result = owner!.workflows.find(candidate => candidate.id === id);
      expect(result, `Generated workflow ${id}`).toBeDefined();
      return result!;
    };
    const expenses = workflow("people_payroll", "people_payroll/feature-expenses");
    expect(expenses.pages).toEqual(expect.arrayContaining([
      "src/app/holdings/[holdingSlug]/_presenters/people/screen.tsx",
      "src/app/holdings/[holdingSlug]/finance/documents/expenses/page.tsx",
      "src/app/holdings/[holdingSlug]/finance/documents/expenses/[id]/page.tsx",
      "src/app/holdings/[holdingSlug]/finance/documents/expenses/new/page.tsx",
      "src/app/holdings/[holdingSlug]/_presenters/expenses/screen.tsx",
      "src/app/holdings/[holdingSlug]/_presenters/expenses/[id]/screen.tsx",
      "src/app/holdings/[holdingSlug]/_presenters/expenses/new/screen.tsx",
    ]));
    expect(expenses.tests).toContain("tests/e2e/people_payroll/expense-v2.spec.ts");

    const payments = workflow("finance", "finance/feature-payments");
    expect(payments.pages).toContain("src/app/holdings/[holdingSlug]/finance/cash/payments/[id]/page.tsx");
    expect(payments.pages).not.toContain("src/app/holdings/[holdingSlug]/finance/documents/sales/invoices/page.tsx");
    expect(payments.tests).not.toContain("tests/e2e/finance/finance-sales-workspace.spec.ts");

    const assets = workflow("assets", "assets/asset-register");
    expect(assets.tests).toContain("tests/integration/finance/finance-assets.test.ts");

    const sales = productAcceptanceRegister.productWorkflows.find(candidate => candidate.id === "finance:sales-invoice-workspace");
    expect(sales).toBeDefined();
    expect(sales!.pages).toEqual(expect.arrayContaining([
      "src/app/holdings/[holdingSlug]/finance/documents/sales/invoices/page.tsx",
      "src/app/holdings/[holdingSlug]/_presenters/finance/sales/screen.tsx",
      "src/app/holdings/[holdingSlug]/finance/documents/sales/new/page.tsx",
      "src/app/holdings/[holdingSlug]/_presenters/finance/sales/new/screen.tsx",
    ]));
    expect(sales!.tests).toEqual(expect.arrayContaining([
      "tests/integration/finance/finance-sales-workspace.test.ts",
      "tests/e2e/finance/finance-sales-workspace.spec.ts",
    ]));
    const executive = productAcceptanceRegister.productWorkflows.find(candidate => candidate.id === "work:company-executive-and-scoped-mcp");
    expect(executive?.tests).toContain("tests/e2e/shared.executive/executive-work-mcp.spec.ts");
  });

  it("maps Talent detail and candidate privacy to their actual owner sources", () => {
    const talent = productAcceptanceRegister.modules.find(module => module.moduleId === "talent");
    expect(talent).toBeDefined();
    const workflow = (id: string) => {
      const result = talent!.workflows.find(candidate => candidate.id === id);
      expect(result, `Generated Talent workflow ${id}`).toBeDefined();
      return result!;
    };
    const detailPages = [
      "src/app/holdings/[holdingSlug]/people-organization/talent/[id]/page.tsx",
      "src/app/holdings/[holdingSlug]/_presenters/talent/[id]/screen.tsx",
      "src/components/records/loaders/modules--talent-console.tsx",
      "src/components/talent/talent-console.tsx",
    ];

    for (const id of ["talent/applications", "talent/offers", "talent/feature-proposals"])
      expect(workflow(id).pages).toEqual(expect.arrayContaining(detailPages));

    const privacy = workflow("talent/feature-candidates-privacy");
    expect(privacy.pages).toEqual(expect.arrayContaining([
      "src/app/holdings/[holdingSlug]/people-organization/talent/candidates/[id]/privacy/page.tsx",
      "src/components/talent/candidate-privacy-page.tsx",
      "src/components/talent/candidate-privacy-editor.tsx",
    ]));
    expect(privacy.apis).toContain("src/app/api/v1/holdings/[holdingSlug]/talent/route.ts");
    expect(privacy.services).toEqual(expect.arrayContaining([
      "src/server/modules/talent/public/candidate-privacy-workspace.ts",
      "src/server/modules/talent/adapters/candidate-privacy-workspace.ts",
      "src/server/modules/talent/public/people-hire-handoff-facts.ts",
      "src/server/modules/talent/adapters/people-hire-handoff-facts.ts",
      "src/server/modules/people_payroll/public/people-handoffs.ts",
      "src/server/modules/people_payroll/adapters/people-handoffs.ts",
      "src/server/modules/people_payroll/application/people-handoffs.ts",
    ]));
    expect(privacy.schema).toContain("src/server/modules/people_payroll/domain/people-handoffs.ts");
    expect(privacy.tests).toContain("tests/unit/learning/learning-talent-services.test.ts");
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
