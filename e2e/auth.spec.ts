import { test, expect } from './fixtures';
import { userIdByCode } from './helpers/db';

test('1. /login renders SignInPanel when signed out', async ({ page }) => {
  await page.goto('/login');
  await expect(page).toHaveURL(/\/login$/);
  await expect(page.getByRole('heading', { name: 'Folio' })).toBeVisible();
});

test('2. /login user grid loads from /api/actor/users', async ({ page }) => {
  await page.goto('/login');
  await expect(page.getByText('Charles Executive')).toBeVisible();
  await expect(page.getByText(/EMP006/)).toBeVisible();
  await expect(page.getByText(/EMP001/)).toBeVisible();
  await expect(page.getByText(/IT001/)).toBeVisible();
  await expect(page.getByText(/EMP019/)).toBeVisible();
  await expect(page.getByText(/EMP005/)).toBeVisible();
});

test('3. Sign in via UI click on a user card', async ({ page }) => {
  await page.goto('/login');
  await page.getByText('Charles Executive').click();
  await page.waitForURL((u) => !u.pathname.startsWith('/login'), { timeout: 15_000 });
  await expect(page).not.toHaveURL(/\/login/);
  const cookies = await page.context().cookies();
  expect(cookies.find((c) => c.name === 'folio_session')).toBeTruthy();
});

test('4. Signed-in user accessing /login redirects to next or /', async ({ page }) => {
  const id = await userIdByCode('EMP006');
  await page.request.post('/api/actor', { data: { id } });
  await page.goto('/login?next=/expense');
  await expect(page).toHaveURL(/\/expense$/);
});

test('5. Visit every protected route while signed out → redirected to /login', async ({ page }) => {
  const routes = [
    '/expense', '/inbox', '/audit', '/policy', '/tiles',
    '/me', '/hr', '/law', '/customers', '/ai-settings',
    '/cockpit', '/sales', '/executive',
  ];
  for (const route of routes) {
    await page.goto(route);
    await expect(page, `route ${route}`).toHaveURL(/\/login/);
  }
});

test('6. /forbidden renders 403 page when accessed directly', async ({ page }) => {
  const id = await userIdByCode('EMP006');
  await page.request.post('/api/actor', { data: { id } });
  const res = await page.goto('/forbidden');
  expect(res?.ok()).toBeTruthy();
  await expect(page).toHaveURL(/\/forbidden$/);
  await expect(page.getByText(/Access|Forbidden|🔒|forbidden/i).first()).toBeVisible();
});

test('7. Legacy redirects (next.config.ts:11)', async ({ page }) => {
  const id = await userIdByCode('EMP006');
  await page.request.post('/api/actor', { data: { id } });

  await page.goto('/dashboard');
  await expect(page).toHaveURL(/\/$/);

  await page.goto('/expense-claim');
  await expect(page).toHaveURL(/\/expense$/);

  await page.goto('/all-approvals');
  await expect(page).toHaveURL(/\/inbox\?scope=all$/);

  await page.goto('/submit-expense');
  await expect(page).toHaveURL(/\/expense$/);
});

test('8. Origin-lookup redirects', async ({ page }) => {
  const r1 = await page.request.get('/expense/123', { maxRedirects: 0 });
  expect([301, 302, 307, 308]).toContain(r1.status());
  expect(r1.headers()['location']).toContain('/waybill/by-expense/123');

  const r2 = await page.request.get('/pr/45', { maxRedirects: 0 });
  expect([301, 302, 307, 308]).toContain(r2.status());
  expect(r2.headers()['location']).toContain('/waybill/by-pr/45');

  const r3 = await page.request.get('/po/67', { maxRedirects: 0 });
  expect([301, 302, 307, 308]).toContain(r3.status());
  expect(r3.headers()['location']).toContain('/waybill/by-po/67');
});

test('9. LangPicker locale switch', async ({ page }) => {
  const id = await userIdByCode('EMP006');
  await page.request.post('/api/actor', { data: { id } });
  await page.goto('/');

  await page.getByRole('button', { name: /Language:/i }).click();
  await page.getByRole('option').filter({ hasText: 'Deutsch' }).click();
  await expect.poll(async () => page.evaluate(() => localStorage.getItem('folio.lang'))).toBe('de');

  await page.getByRole('button', { name: /Language:/i }).click();
  await page.getByRole('option').filter({ hasText: 'ไทย' }).click();
  await expect.poll(async () => page.evaluate(() => localStorage.getItem('folio.lang'))).toBe('th');
});

test('10. Me page renders user profile when signed in', async ({ page }) => {
  const id = await userIdByCode('EMP001');
  await page.request.post('/api/actor', { data: { id } });
  await page.goto('/me');
  await expect(page.locator('body')).toContainText(/John Staff|EMP001/, { timeout: 15_000 });
});