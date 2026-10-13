import {expect,test} from '@playwright/test';
import { selectAcmeHoldingOwner } from "../../support/browser/select-acme-holding-owner";

test('inventory screens expose traceability and separate stock positions',async({context,page},testInfo)=>{
  const login=await context.request.post('/api/auth/sign-in/email',{data:{email:process.env.ERPX_E2E_ADMIN_EMAIL??'jesadakorn.kirtnu@gmail.com',password:process.env.ERPX_E2E_ADMIN_PASSWORD??'12345678'}});
  expect(login.ok()).toBe(true);
  await selectAcmeHoldingOwner(page);

  const sku=`LOT-${testInfo.project.name.slice(0,3).toUpperCase()}-${Date.now().toString(36)}`;
  await page.goto('/holdings/acme/commerce?view=products&new=1');
  const workspace=page.locator('#workspace-content:visible');
  await expect(workspace).toHaveCount(1);
  await expect(workspace.getByRole('heading',{name:'Products',exact:true,level:1})).toBeVisible();
  await workspace.getByLabel('SKU').fill(sku);
  await workspace.getByLabel('Name').fill('Browser acceptance lot item');
  await workspace.getByLabel('Unit').fill('box');
  await workspace.getByLabel('Traceability').selectOption('lot');
  await workspace.getByLabel('List price').fill('1200');
  await workspace.getByLabel('Default tax (%)').fill('7');
  await workspace.getByRole('button',{name:'Save product'}).click();
  const row=workspace.getByRole('row').filter({hasText:sku});
  await expect(row).toBeVisible();
  await expect(row).toContainText('lot tracked');

  await page.goto('/holdings/acme/commerce?view=stock');
  for(const header of ['On hand','Reserved','Quarantined','In transit','Available'])await expect(workspace.getByRole('columnheader',{name:header})).toBeVisible();

  await page.goto('/holdings/acme/quality');
  await expect(workspace.getByRole('heading',{name:'Quality holds'})).toBeVisible();
  await expect(workspace.getByLabel('Product, lot / serial, and warehouse')).toBeVisible();
});
