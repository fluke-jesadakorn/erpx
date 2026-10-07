import { expect, test } from '@playwright/test';
import { selectFixtureAssignment } from './helpers/select-fixture-assignment';
const base = '/holdings/acme';
test.beforeEach(async ({ context, baseURL }) => {
  test.skip(!['localhost', '127.0.0.1'].includes(new URL(baseURL!).hostname), 'Uses the existing local test account.');
  const response = await context.request.post('/api/auth/sign-in/email', { data: { email: 'mali@erpx.dev', password: 'Mali-ERPX-2026!' } });
  expect(response.status()).toBe(200);
  await selectFixtureAssignment(context, 'acme-co', 'seed_usr_mali');
});

test('retired pages render fixed widgets without layout reads or controls', async ({ page }) => {
  test.setTimeout(180000);
  const layoutRequests: string[] = [];
  page.on('request', request => { if (request.url().includes('/page-layouts/')) layoutRequests.push(request.url()); });
  for (const route of ['home', 'work', 'inbox', 'calendar', 'handbook', 'learn', 'expenses']) {
    await page.goto(`${base}/${route}`);
    await expect(page.locator('main h1').first()).toBeVisible({ timeout: 30000 });
    await expect(page.getByRole('button', { name: 'Customize layout', exact: true })).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Edit organization layout', exact: true })).toHaveCount(0);
    await expect(page.locator('main [data-layout-controls]')).toHaveCount(0);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), route).toBe(true);
  }
  expect(layoutRequests).toEqual([]);
});

test('People layout previews width, order and visibility and cancel restores the page', async ({ page }) => {
  await page.goto(`${base}/overtime`);
  const current = page.locator('[data-feature-widget="overtime-requests-history"]');
  await expect(page.getByRole('button', { name: 'Customize layout', exact: true })).toBeEnabled({ timeout: 30000 });
  const mutations: string[] = [];
  page.on('request', request => { if (request.method() === 'POST' && request.url().includes('/api/v1')) mutations.push(request.url()); });
  await page.getByRole('button', { name: 'Customize layout', exact: true }).click();
  await page.getByRole('combobox', { name: 'Requests & history width', exact: true }).selectOption('6');
  await expect(current).toHaveAttribute('style', /grid-column: span 6/);
  await page.getByRole('button', { name: 'Move Requests & history', exact: true }).press('ArrowDown');
  await page.getByRole('button', { name: 'Hide Requests & history', exact: true }).click();
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(current).toBeVisible();
  await expect(page.locator('[data-feature-widget="overtime-requests-history"]')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Customize layout', exact: true })).toBeEnabled();
  expect(mutations).toEqual([]);
});

test('My work views remain reachable through sidebar or mobile submenu', async ({ page }, testInfo) => {
  test.setTimeout(180000);
  await page.goto(`${base}/work`);
  await expect(page.locator('main h1')).toHaveText('My work');
  await expect(page.getByRole('navigation', { name: 'Work queues', exact: true })).toHaveCount(0);
  await expect(page.getByText('More views', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Search workspace', exact: true })).toBeEnabled();
  const mobile = testInfo.project.name === 'mobile';
  for (const [view, label] of [['actions', 'My actions'], ['approvals', 'Approvals'], ['tracking', 'Tracking'], ['for-me', 'For me'], ['completed', 'Completed'], ['waiting', 'Waiting'], ['requests', 'My requests'], ['all', 'All work']]) {
    if (mobile) await page.getByRole('navigation', { name: 'Mobile navigation', exact: true }).getByRole('link', { name: /My work/ }).click();
    const submenu = mobile ? page.getByRole('dialog', { name: 'My work views', exact: true }) : page.locator('[aria-label="My work views"]');
    await submenu.getByRole('link', { name: label, exact: true }).click();
    await expect(page).toHaveURL(new RegExp(`view=${view}$`), { timeout: 30000 });
    await expect(page.locator('main h1')).toHaveText('My work');
    if (!mobile) await expect(submenu.getByRole('link', { name: label, exact: true })).toHaveAttribute('aria-current', 'page');
  }
});
