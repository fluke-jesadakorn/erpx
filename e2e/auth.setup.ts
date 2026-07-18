import { test, expect } from './fixtures';
import { pinLocale } from './helpers/lang';

const PERSONA_KEYS = [
  'officerEmp001',
  'managerEmp002',
  'accountOfficerEmp003',
  'accountingManagerEmp004',
  'cfoEmp005',
  'ceoEmp006',
  'supervisorEmp017',
  'accountSupervisorEmp018',
  'financeEmp019',
  'hrManagerEmp015',
  'hrOfficerEmp016',
  'itAdminIT001',
  'hrManagerHR002',
  'lawParalegalLW001',
  'lawCounselLW002',
  'salesOfficer',
] as const;

for (const key of PERSONA_KEYS) {
  test(`auth.setup → ${key}`, async ({ page }) => {
    await pinLocale(page.context(), 'th');
    await page.goto('/login');
    await test.step(`sign in as ${key}`, async () => {
      const persona = (await import('./fixtures')).PERSONAS[key];
      const { userIdByCode } = await import('./helpers/db');
      const id = await userIdByCode(persona.code);
      const r = await page.request.post('/api/actor', { data: { id } });
      expect(r.ok()).toBeTruthy();
    });
    await page.goto('/');
    await page.waitForURL(/\/(login|$)/);
    await expect(page).not.toHaveURL(/\/login/);
  });
}
