import { test, expect } from '@playwright/test';

test('stale company preference recovers holding access without allowing unauthorized company selection', async ({ page, context, baseURL }) => {
  const login = await context.request.post('/api/auth/sign-in/email', { data: { email: 'mali@erpx.dev', password: 'Mali-ERPX-2026!' } });
  expect(login.ok()).toBe(true);
  // Beacon is a real active company outside Mali's holding-level assignment.
  await context.addCookies([{name:'erpx_company_h001',value:'beacon-co',url:baseURL!}]);
  await page.goto('/holdings/acme/home');
  await expect(page.getByRole('heading', {name:'Hello, Mali'})).toBeVisible({timeout:30000});
  await expect(page).toHaveURL(/\/holdings\/acme\/home$/);
  const denied = await context.request.post('/api/v1/holdings/acme/companies/active', {data:{companyId:'beacon-co'}});
  expect(denied.status()).toBe(403);
  await page.getByRole('button',{name:'Switch workspace',exact:true}).click();
  await expect(page.getByRole('dialog')).not.toContainText('Beacon Labs');
  await expect(page.getByRole('dialog')).not.toContainText('Expense test');
});
