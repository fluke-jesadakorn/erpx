import { describe, expect, it } from "vitest";

import { generalCompanyDemoGuide, type DemoActor } from "../scripts/fixtures/general-company-demo";

describe("general company working demo", () => {
  it("documents a resettable real workflow with independent business roles", () => {
    const actor = (role: string): DemoActor => ({ label: role, email: `${role}@simulation.test`, userId: `user-${role}`, role });
    const guide = generalCompanyDemoGuide({
      run: "0123456789ab",
      origin: "http://127.0.0.1:3102",
      holdingSlug: "simulation",
      requestId: "request-1",
      quotationId: "quote-1",
      lotProductId: "product-lot",
      serialProductId: "product-serial",
      customerId: "customer-1",
      customerOperator: actor("sales_operator"),
      qualityDueSoonHoldId: "quality-due-soon",
      qualityOverdueHoldId: "quality-overdue",
      qualityInspector: actor("quality_inspector"),
      qualityManager: actor("quality_manager"),
      closePeriodIds: ["period-jan", "period-feb"],
      controlledPaymentInvoiceId: "invoice-payment-demo",
      actors: {
        employee: actor("employee"), manager: actor("people_manager"), hrApprover: actor("hr_admin"),
        payrollPreparer: actor("payroll_preparer"), buyer: actor("purchasing_operator"),
        purchasingManager: actor("purchasing_manager"), salesOperator: actor("sales_operator"), salesManager: actor("sales_manager"), warehouse: actor("warehouse_manager"),sourceWarehouse:actor("warehouse_manager"),
        financePreparer: actor("finance_preparer"), financeApprover: actor("finance_approver"),
        treasury: actor("treasury"), administrator: actor("holding_owner"),
      },
    });
    expect(guide.environment.kind).toBe("isolated-real-workflow");
    expect(guide.environment.externalIntegrations).toContain("no email, bank, payment, or provider action is live");
    expect(guide.environment.controls.supplierInvoiceMatching).toBe("required");
    expect(guide.reset.commands).toEqual(["bun run demo:destroy -- 0123456789ab", "bun run demo:prepare"]);
    expect(guide.acceptance).toMatchObject({ command: "bun run demo:verify", test: "tests/purchase-to-payment.test.ts" });
    const scenario = guide.scenarios[0];
    expect(scenario.sourceRecord).toEqual({ type: "employee_request", id: "request-1" });
    expect(scenario.steps.map(step => step.order)).toEqual([...Array(scenario.steps.length)].map((_, index) => index + 1));
    expect(new Set(scenario.steps.map(step => step.actor))).toEqual(new Set(["employee", "manager", "financePreparer", "financeApprover", "buyer", "purchasingManager", "warehouse", "treasury"]));
    expect(scenario.steps.at(-1)?.href).toBe("http://127.0.0.1:3102/holdings/simulation/requests/request-1");
    const split = guide.scenarios[1];
    expect(split.id).toBe("split-requisition-and-unit-conversion");
    expect(split.steps.some(step=>step.expected.includes("จัดสรรรวมเกิน 10 ไม่ได้"))).toBe(true);
    const sales = guide.scenarios[3];
    expect(sales).toMatchObject({ id: "sales-to-collection", sourceRecord: { type: "commerce_quote", id: "quote-1" } });
    expect(sales.steps.map(step => step.actor)).toEqual(["salesOperator","salesManager","salesOperator","salesManager","warehouse","financeApprover","financePreparer","financeApprover","treasury"]);
    const close = guide.scenarios[2];
    expect(close).toMatchObject({ id: "finance-two-consecutive-closes", sourceRecord: { type: "finance_period", id: "period-jan" } });
    expect(close.steps.map(step=>step.actor)).toEqual(["financePreparer","financePreparer","financeApprover","financePreparer","financeApprover"]);
    const payment = guide.scenarios[4];
    expect(payment).toMatchObject({ id: "finance-controlled-bank-payment", sourceRecord: { type: "finance_invoice", id: "invoice-payment-demo" } });
    expect(payment.steps.map(step=>step.actor)).toEqual(["financePreparer","financeApprover","treasury","treasury"]);
    const traceable = guide.scenarios[5];
    expect(traceable).toMatchObject({ id: "traceable-receipt-and-quality-release", products: { lotTracked: "product-lot", serialTracked: "product-serial" } });
    expect(traceable.steps.map(step=>step.order)).toEqual([1,2,3,4,5,6]);
    const quality = guide.scenarios.find(s => s.id === "quality-escalation-recovery");
    expect(quality).toMatchObject({ id: "quality-escalation-recovery", sourceRecord: { type: "quality_alerts", id: "quality-due-soon" } });
    expect(quality?.steps.map(step=>step.actor)).toEqual(["warehouse", "qualityInspector", "qualityInspector", "qualityManager", "administrator"]);
    const timeline = guide.scenarios.find(s => s.id === "customer-360-scoped-timeline");
    expect(timeline).toMatchObject({ id: "customer-360-scoped-timeline", sourceRecord: { type: "finance_party", id: "customer-1" } });
    expect(timeline?.steps.map(step=>step.actor)).toEqual(["customerOperator", "customerOperator", "salesManager", "customerOperator"]);
    const overtime = guide.scenarios.find(s => s.id === "employee-overtime-to-payroll");
    expect(overtime?.startingState).toContain("No overtime request exists until the employee creates it");
    expect(overtime?.steps.map(step=>step.actor)).toEqual(["employee", "manager", "hrApprover", "payrollPreparer"]);
    expect(overtime?.steps.every(step=>step.expected.length > 0 && step.href.startsWith("http://127.0.0.1:3102/"))).toBe(true);
  });
});
