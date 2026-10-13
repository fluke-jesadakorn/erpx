import { test, expect } from "@playwright/test";
import { selectFixtureAssignment } from "../../support/browser/select-fixture-assignment";

test("a signed-in shift worker sees My shifts",async({page,context})=>{

  const login=await context.request.post("/api/auth/sign-in/email",{data:{email:process.env.ERPX_E2E_READER_EMAIL??"elena.suwan@erpx.dev",password:process.env.ERPX_E2E_READER_PASSWORD??"12345678"}});
  expect(login.ok()).toBe(true);
  const { user } = await login.json() as { user: { id: string } };
  expect(user.id).toBe("demo-org-v1-bangkok-operations-staff-user");
  // This worker has a department assignment, so select it in that workspace.
  const assignmentApi = "/api/v1/holdings/acme/branches/bangkok/departments/operations/assignments/active";
  const assignmentId = "demo-org-v1-bangkok-operations-staff-employee";
  const current = await context.request.get(assignmentApi);
  expect(current.ok(), await current.text()).toBe(true);
  const { data } = await current.json() as { data: { userId: string; companyId: string; revision: number; options: { id: string; companyId: string; scopeKind: string; scopeNodeId: string }[] } };
  expect(data).toMatchObject({ userId: user.id, companyId: "acme-co" });
  expect(Number.isInteger(data.revision) && data.revision >= 0).toBe(true);
  expect(data.options.filter(option => option.id === assignmentId)).toEqual([
    expect.objectContaining({ id: assignmentId, companyId: "acme-co", scopeKind: "department", scopeNodeId: "dept:bangkok:operations" }),
  ]);
  const selected = await context.request.post(assignmentApi, {
    headers: { Origin: process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102" },
    data: { assignmentId, revision: data.revision },
  });
  expect(selected.ok(), await selected.text()).toBe(true);
  const selection = (await selected.json()).data;
  expect(selection).toMatchObject({ userId: user.id, companyId: "acme-co", assignmentId, appliesToWorkspace: true });
  const persisted = await context.request.get(assignmentApi);
  expect(persisted.ok(), await persisted.text()).toBe(true);
  expect((await persisted.json()).data).toMatchObject({ valid: true, userId: user.id, companyId: "acme-co", assignmentId, revision: selection.revision });
  const path="/holdings/acme/branches/bangkok/departments/operations/people/roster";
  await page.goto(path);
  await expect(page.getByRole("heading",{name:"My shifts",exact:true}).first()).toBeVisible();
  await expect(page).toHaveURL(/\/people\?.*panel=schedule/);
  await page.getByRole("combobox",{name:"Schedule view",exact:true}).selectOption("team");
  await expect(page.getByRole("heading",{name:"Team roster",exact:true})).toBeVisible();
  await expect(page.getByText("Morning",{exact:true}).first()).toBeVisible();
  await page.getByRole("button",{name:"Back to structure",exact:true}).click();
  await expect(page.getByTestId("people-structure-workspace")).toBeVisible();
  await expect(page.getByTestId("people-detail")).toHaveCount(0);
  await expect(page).not.toHaveURL(/panel=schedule/);
});

test("a signed-in backoffice manager sees team management without My shifts",async({page,context})=>{

  const login=await context.request.post("/api/auth/sign-in/email",{data:{email:"mali@erpx.dev",password:"Mali-ERPX-2026!"}});
  expect(login.ok()).toBe(true);
  await selectFixtureAssignment(context, "acme-co", "seed_usr_mali");
  const path="/holdings/acme/branches/bangkok/departments/operations/people/roster?view=mine";
  await page.goto(path);
  await expect(page.getByRole("heading",{name:"Team roster",exact:true})).toBeVisible();
  await expect(page).toHaveURL(/\/people\?.*panel=schedule/);
  await expect(page.getByRole("heading",{name:"My shifts",exact:true})).toHaveCount(0);
  await expect(page.getByRole("button",{name:"Add shift"}).first()).toBeVisible();
});
