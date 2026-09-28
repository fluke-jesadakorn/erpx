import { test, expect } from "@playwright/test";

test("a signed-in shift worker sees My shifts",async({page,context},info)=>{
  test.skip(info.project.name!=="desktop","The roster layout is also covered by responsive checks.");
  const login=await context.request.post("/api/auth/sign-in/email",{data:{email:"elena.suwan@erpx.dev",password:"12345678"}});
  expect(login.ok()).toBe(true);
  const path="/holdings/acme/branches/bangkok/departments/operations/people/roster";
  await page.goto(path);
  if(page.url().includes("/work-role")) {
    await page.getByRole("combobox",{name:"Role for actions & approvals"}).fill("employee");
    await page.getByRole("listbox").getByRole("option").first().click();
    await page.goto(path);
  }
  await expect(page.getByRole("heading",{name:"My shifts",exact:true}).first()).toBeVisible();
  await expect(page.getByRole("heading",{name:"Team roster",exact:true})).toBeVisible();
  await expect(page.getByText("Morning",{exact:true}).first()).toBeVisible();
});

test("a signed-in backoffice manager sees team management without My shifts",async({page,context},info)=>{
  test.skip(info.project.name!=="desktop","The roster layout is also covered by responsive checks.");
  const login=await context.request.post("/api/auth/sign-in/email",{data:{email:"mali@erpx.dev",password:"Mali-ERPX-2026!"}});
  expect(login.ok()).toBe(true);
  const path="/holdings/acme/branches/bangkok/departments/operations/people/roster?view=mine";
  await page.goto(path);
  if(page.url().includes("/work-role")) {
    await page.getByRole("combobox",{name:"Role for actions & approvals"}).fill("people manager");
    await page.getByRole("listbox").getByRole("option").first().click();
    await page.goto(path);
  }
  await expect(page.getByRole("heading",{name:"Team roster",exact:true})).toBeVisible();
  await expect(page.getByRole("heading",{name:"My shifts",exact:true})).toHaveCount(0);
  await expect(page.getByRole("button",{name:"Add shift"}).first()).toBeVisible();
});
