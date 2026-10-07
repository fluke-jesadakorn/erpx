import { test, expect } from "@playwright/test";
import { selectFixtureAssignment } from "./helpers/select-fixture-assignment";

test("a signed-in shift worker sees My shifts",async({page,context})=>{

  const login=await context.request.post("/api/auth/sign-in/email",{data:{email:process.env.ERPX_E2E_READER_EMAIL??"elena.suwan@erpx.dev",password:process.env.ERPX_E2E_READER_PASSWORD??"12345678"}});
  expect(login.ok()).toBe(true);
  await selectFixtureAssignment(context, "acme-co", "demo-org-v1-bangkok-operations-staff-employee", "acme", "/branches/bangkok/departments/operations");
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
