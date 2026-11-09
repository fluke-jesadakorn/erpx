import { expect, type BrowserContext } from "@playwright/test";

/** Persist the same Company and work-role selection used by the application. */
export async function selectFixtureAssignment(context: BrowserContext, companyId: string, assignmentId: string, holdingSlug = "acme", workspaceSuffix = "") {
  const api = `/api/v1/holdings/${holdingSlug}`, origin = process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102";
  const assignmentApi = `${api}${workspaceSuffix}/assignments/active`;
  if (process.env.ERPX_E2E_LOOPBACK_SSO === "1" && !(await context.cookies(origin)).some(cookie => cookie.name === "__Host-erpx-product")) {
    const sso = await context.request.get(`${origin}/sso/start?next=${encodeURIComponent(`/holdings/${holdingSlug}/home`)}`);
    expect(sso.status(), await sso.text()).toBeLessThan(500);
    expect((await context.cookies(origin)).some(cookie => cookie.name === "__Host-erpx-product"), "Native Holding SSO must establish the product session").toBeTruthy();
  }
  const company = await context.request.post(`${api}/companies/active`, { headers: { Origin: origin }, data: { companyId } });
  expect(company.ok(), await company.text()).toBeTruthy();
  const current = await context.request.get(assignmentApi);
  expect(current.ok(), await current.text()).toBeTruthy();
  const { data } = await current.json() as { data: { revision: number; companyId: string; options: { id: string }[] } };
  expect(data.companyId).toBe(companyId);
  expect(data.options.some(option => option.id === assignmentId)).toBeTruthy();
  const selected = await context.request.post(assignmentApi, { headers: { Origin: origin }, data: { assignmentId, revision: data.revision } });
  expect(selected.ok(), await selected.text()).toBeTruthy();
  const persisted = await context.request.get(assignmentApi);
  expect(persisted.ok(), await persisted.text()).toBeTruthy();
  expect((await persisted.json()).data).toMatchObject({ valid: true, companyId, assignmentId });
}
