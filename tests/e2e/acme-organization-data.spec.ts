import { test, expect } from "@playwright/test";
import { demoStaffPlan, demoInitialPassword } from "../../scripts/fixtures/acme-organization";

const plan = demoStaffPlan({ personId: "hr-acme-co-001", positionId: "existing-ceo" });
const seats = ["operations-staff", "operations-supervisor", "operations-director", "accounting-senior", "accounting-supervisor", "accounting-manager", "finance-staff", "people-manager", "purchasing-staff", "warehouse-manager", "sales-staff", "it-manager"];
test("expanded Acme accounts sign in with scoped charts and spending readiness", async ({ playwright, baseURL }, testInfo) => {
  test.skip(testInfo.project.name !== "desktop", "API authentication does not depend on viewport.");
  test.setTimeout(120_000);
  for (const seat of seats) {
    const person = plan.find(p => p.key === `demo-org-v1-bangkok-${seat}`)!;
    const context = await playwright.request.newContext({ baseURL });
    try {
      const login = await context.post("/api/auth/sign-in/email", { data: { email: person.email, password: demoInitialPassword(person) } });
      expect(login.ok(), person.email).toBe(true);
      const department = person.departmentId.split(":").at(-1);
      const api = `/api/v1/holdings/acme/branches/bangkok/departments/${department}/people/organization`;
      const overview = await context.get(api);
      expect(overview.ok(), person.email).toBe(true);
      const { data } = await overview.json();
      expect(data.people.some((p: { user_id: string }) => p.user_id === person.userId)).toBe(true);
      expect(data.people.every((p: { branch_id: string; department_id: string }) => p.branch_id === "bangkok" && p.department_id === person.departmentId)).toBe(true);
      const readiness = await context.get(api + "/readiness?kind=purchase");
      expect(readiness.ok()).toBe(true);
      expect((await readiness.json()).data).toMatchObject({ ready: true, blockedReason: null });
      // Ordinary staff has no authority over the company or another department.
      if (seat === "operations-staff") {
        expect((await context.get("/api/v1/holdings/acme/people/organization")).status()).toBe(403);
        expect((await context.get("/api/v1/holdings/acme/branches/bangkok/departments/accounting/people/organization")).status()).toBe(403);
      }
    } finally { await context.dispose(); }
  }
});
