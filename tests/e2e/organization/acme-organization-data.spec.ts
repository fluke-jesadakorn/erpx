import { test, expect } from "@playwright/test";
import { demoStaffPlan, demoInitialPassword } from "../../../fixtures/organization/builders/acme-organization";

const plan = demoStaffPlan({ personId: "hr-acme-co-001", positionId: "existing-ceo" });
const seats = ["operations-staff", "operations-supervisor", "operations-director", "accounting-senior", "accounting-supervisor", "accounting-manager", "finance-staff", "people-manager", "purchasing-staff", "warehouse-manager", "sales-staff", "it-manager"];
test("expanded Acme accounts sign in with scoped charts and spending readiness", async ({ playwright, baseURL }) => {

  test.setTimeout(120_000);
  for (const seat of seats) {
    const person = plan.find(p => p.key === `demo-org-v1-bangkok-${seat}`)!;
    const context = await playwright.request.newContext({ baseURL });
    try {
      const login = await context.post("/api/auth/sign-in/email", { data: { email: person.email, password: demoInitialPassword(person) } });
      expect(login.ok(), person.email).toBe(true);
      const department = person.departmentId.split(":").at(-1);
      const workspaceApi = `/api/v1/holdings/acme/branches/bangkok/departments/${department}`;
      const origin = new URL(baseURL!).origin;
      const assignmentApi = `${workspaceApi}/assignments/active`;
      const current = await context.get(assignmentApi);
      expect(current.ok(), person.email).toBe(true);
      const { data: selection } = await current.json() as { data: { revision: number; companyId: string; userId: string; options: { id: string }[] } };
      const assignmentId = `${person.key}-employee`;
      expect(selection).toMatchObject({ companyId: "acme-co", userId: person.userId });
      expect(selection.options.some(option => option.id === assignmentId)).toBe(true);
      const selected = await context.post(assignmentApi, { headers: { Origin: origin }, data: { assignmentId, revision: selection.revision } });
      expect(selected.ok(), person.email).toBe(true);
      const persisted = await context.get(assignmentApi);
      expect(persisted.ok(), person.email).toBe(true);
      expect((await persisted.json()).data).toMatchObject({ valid: true, companyId: "acme-co", userId: person.userId, assignmentId });
      const api = `${workspaceApi}/people/organization`;
      const overview = await context.get(`${workspaceApi}/staff/people?departmentId=${encodeURIComponent(person.departmentId)}`);
      expect(overview.ok(), person.email).toBe(true);
      const { data: people } = await overview.json();
      const data = { people };
      expect(data.people.some((p: { user_id: string }) => p.user_id === person.userId)).toBe(true);
      expect(data.people.every((p: { branch_id: string; department_id: string }) => p.branch_id === "bangkok" && p.department_id === person.departmentId)).toBe(true);
      const readiness = await context.get(api + "/readiness?kind=purchase");
      expect(readiness.ok()).toBe(true);
      expect((await readiness.json()).data).toMatchObject({ ready: true, blockedReason: null });
      const administration = await context.get(api);
      // The fixture appoints Lara as Company HR; the appointment applies to her employee role too.
      if (seat === "people-manager") {
        expect(administration.status(), person.email).toBe(200);
        const { data } = await administration.json();
        expect(data.people.some((p: { user_id: string }) => p.user_id === person.userId)).toBe(true);
        expect(data.people.every((p: { branch_id: string; department_id: string }) => p.branch_id === "bangkok" && p.department_id === person.departmentId)).toBe(true);
      } else {
        expect(administration.status(), person.email).toBe(403);
      }
      // Ordinary staff has no authority over the company or another department.
      if (seat === "operations-staff") {
        const companyOrganization = await context.get("/api/v1/holdings/acme/people/organization");
        expect(companyOrganization.status()).toBe(409);
        expect(await companyOrganization.json()).toEqual({ error: { code: "COMPANY_REQUIRED", message: "Select an active Company to continue." } });
        expect((await context.get("/holdings/acme/home")).status()).toBe(403);
        const foreignOrganization = await context.get("/api/v1/holdings/acme/branches/bangkok/departments/accounting/people/organization");
        expect(foreignOrganization.status()).toBe(409);
        expect(await foreignOrganization.json()).toEqual({ error: { code: "WORK_ROLE_UNAVAILABLE", message: "Your selected role no longer covers this workspace. Choose an available role." } });
        expect((await context.get("/holdings/acme/branches/bangkok/departments/accounting/home")).status()).toBe(403);
      }
    } finally { await context.dispose(); }
  }
});
