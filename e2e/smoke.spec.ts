import { test, expect } from './fixtures';
import { userIdByCode } from './helpers/db';

test('dev server is reachable on /login', async ({ page }) => {
  const r = await page.goto('/login');
  expect(r?.ok()).toBeTruthy();
  await expect(page).toHaveURL(/\/login/);
});

test('POST /api/actor signs in a seeded user', async ({ page }) => {
  const id = await userIdByCode('EMP006');
  const r = await page.request.post('/api/actor', { data: { id } });
  expect(r.ok()).toBeTruthy();
  const cookies = await page.context().cookies();
  expect(cookies.find(c => c.name === 'folio_session')).toBeTruthy();
});

test('DELETE /api/actor signs out', async ({ page }) => {
  const id = await userIdByCode('EMP006');
  await page.request.post('/api/actor', { data: { id } });
  const r = await page.request.delete('/api/actor');
  expect(r.ok()).toBeTruthy();
  const cookies = await page.context().cookies();
  expect(cookies.find(c => c.name === 'folio_session')).toBeFalsy();
});

test('GET /api/actor/users returns seeded users', async ({ page }) => {
  const id = await userIdByCode('EMP006');
  await page.request.post('/api/actor', { data: { id } });
  const r = await page.request.get('/api/actor/users');
  expect(r.ok()).toBeTruthy();
  const body = await r.json() as { users: Array<{ employee_code: string }>; pinRequired: boolean };
  expect(body.users.length).toBeGreaterThan(10);
  expect(body.users.find(u => u.employee_code === 'EMP006')).toBeTruthy();
});
