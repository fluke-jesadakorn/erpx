import { randomUUID } from "node:crypto";
import { expect, type BrowserContext } from "@playwright/test";
import { selectFixtureAssignment } from "../e2e/helpers/select-fixture-assignment";

type AuthoritySeat = {
  company_id: string;
  kind: string;
  scope_id: string;
  version: number;
  person_id: string | null;
};
type FixturePerson = {
  id: string;
  company_id: string;
  user_id: string | null;
  legal_name: string;
  status: string;
};

/** Restore the legacy Leave scenario's declared HR reviewer in its own fresh cohort. */
export async function prepareLegacyLeaveReviewer(context: BrowserContext): Promise<void> {
  try {
    if (!/^[a-f0-9]{12}$/.test(process.env.ERPX_BROWSER_SIMULATION_RUN ?? ""))
      throw new Error("DISPOSABLE_BROWSER_COHORT_REQUIRED");
    const origin = new URL(process.env.ERPX_E2E_BASE_URL ?? "");
    if (origin.protocol !== "http:" || !["127.0.0.1", "localhost"].includes(origin.hostname)
      || !["3102", "3104"].includes(origin.port) || origin.username || origin.password
      || origin.pathname !== "/" || origin.search || origin.hash)
      throw new Error("EXACT_LEGACY_LEAVE_BROWSER_ORIGIN_REQUIRED");
    if ((await context.cookies()).length)
      throw new Error("FRESH_LEGACY_LEAVE_BROWSER_CONTEXT_REQUIRED");

    const page = await context.newPage();
    await page.goto(`${origin.origin}/login`);
    await page.getByLabel("Work email").fill("mali@erpx.dev");
    await page.getByLabel(/^Password(?:\s*\*)?$/).fill("Mali-ERPX-2026!");
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await expect(page).toHaveURL(/\/holdings\/acme(?:\/branches\/bangkok)?\/home$/);
    await selectFixtureAssignment(context, "acme-co", "asg_mali_owner");

    const api = `${origin.origin}/api/v1/holdings/acme`;
    const selection = await context.request.get(`${api}/assignments/active`);
    expect(selection.ok(), await selection.text()).toBe(true);
    const { data: current } = await selection.json() as {
      data: { valid: boolean; companyId: string; userId: string; assignmentId: string; revision: number };
    };
    expect(current).toMatchObject({ valid: true, companyId: "acme-co", userId: "usr_mali", assignmentId: "asg_mali_owner" });
    const headers = {
      Origin: origin.origin,
      "x-erpx-context-company": current.companyId,
      "x-erpx-context-assignment": current.assignmentId,
      "x-erpx-context-revision": String(current.revision),
    };
    const authority = await context.request.get(`${api}/authority`, { headers });
    expect(authority.ok(), await authority.text()).toBe(true);
    const { data: before } = await authority.json() as { data: { seats: AuthoritySeat[] } };
    const seats = before.seats.filter(seat => seat.company_id === "acme-co" && seat.kind === "hr" && seat.scope_id === "acme-co");
    expect(seats, "The isolated Company has one current HR appointment seat").toHaveLength(1);
    expect(Number.isSafeInteger(seats[0].version) && seats[0].version >= 0).toBe(true);

    const people = await context.request.get(`${api}/people/people/hr-acme-co-003`, { headers });
    expect(people.ok(), await people.text()).toBe(true);
    const { data: { person: reviewer } } = await people.json() as { data: { person: FixturePerson } };
    expect(reviewer, "Ava is the existing active legacy HR fixture person").toMatchObject({
      id: "hr-acme-co-003", company_id: "acme-co", user_id: "usr_ava", legal_name: "Ava Williams", status: "active",
    });

    const appointment = await context.request.post(`${api}/authority/appointments`, {
      headers: { ...headers, "Idempotency-Key": randomUUID() },
      data: {
        kind: "hr", departmentId: null, positionId: null, personId: reviewer.id,
        expectedVersion: seats[0].version,
        reason: "Declared disposable legacy Leave browser prerequisite: retain Mali, Nisa and Ava as independent scenario participants.",
      },
    });
    expect(appointment.ok(), await appointment.text()).toBe(true);
    const persisted = await context.request.get(`${api}/authority`, { headers });
    expect(persisted.ok(), await persisted.text()).toBe(true);
    const { data: after } = await persisted.json() as { data: { seats: AuthoritySeat[] } };
    expect(after.seats.filter(seat => seat.kind === "hr" && seat.scope_id === "acme-co")).toEqual([
      expect.objectContaining({ company_id: "acme-co", person_id: reviewer.id, version: seats[0].version + 1 }),
    ]);
  } finally {
    await context.close();
  }
}
