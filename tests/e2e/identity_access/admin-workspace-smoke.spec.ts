import { expect, test } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { Pool } from 'pg';
import { browserProjects } from '../../../scripts/testing/browser-results';

test('Admin workspace signs in through Account and opens its isolated brand drawer without changing authority', browserProjects(['desktop'], 'Single forward development smoke; production browser matrix remains deferred.'), async ({ page, context }, info) => {
  test.setTimeout(240_000);
  const run = process.env.ERPX_DEMO_RUN;
  expect(process.env.ERPX_BROWSER_EXECUTION_MODE).toBe('development-smoke');
  expect(process.env.ERPX_DEVELOPMENT_HOST_LEASE).toBeTruthy();
  expect(run).toMatch(/^[a-f0-9]{12}$/);
  const directory = join(process.cwd(), '.runtime/browser', run!);
  const read = async (name: string) => JSON.parse(await readFile(join(directory, name), 'utf8'));
  const state = await read('state.json'), credentials = await read('credentials.json');
  expect(state.run).toBe(run);
  const owner = credentials.users.find((user: { role: string; email: string }) => user.role === 'holding_owner' && user.email === 'source@simulation.test');
  expect(owner).toBeTruthy();
  const manifest = await read('databases.json'), connection = new URL(manifest.databases.H001_DB);
  expect([connection.hostname, connection.port, connection.username, connection.pathname]).toEqual(['127.0.0.1', '5432', 'erpx_test_admin', `/erpx_browser_${run}_h001_db`]);
  const db = new Pool({ connectionString: connection.href, max: 1 });
  const authority = async () => ({
    assignments: (await db.query('SELECT * FROM assignments WHERE company_id=$1 AND user_id=$2 ORDER BY id', [state.fixture.companyId, owner.userId])).rows,
    packages: (await db.query('SELECT * FROM company_package_grants WHERE company_id=$1 ORDER BY package_id', [state.fixture.companyId])).rows,
  });
  try {
    const before = await authority(), errors: string[] = [], origin = 'https://admin-dev-erpx.jesadakorn.com';
    expect(before.packages).toEqual([]);
    page.on('pageerror', error => errors.push(error.message));
    const login = await page.goto(origin + '/?workspaceSmoke=preserved', { timeout: 120_000 });
    expect(new URL(page.url()).origin).toBe('https://account-dev-erpx.jesadakorn.com');
    expect(login?.headers()['x-erpx-app']).toBe('account');
    await page.getByRole('textbox', { name: 'Work email', exact: true }).fill(owner.email);
    await page.locator('input[name="password"]').fill(credentials.password);
    await page.getByRole('button', { name: 'Sign in', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Choose a workspace', exact: true })).toBeVisible({ timeout: 90_000 });
    expect(new URL(page.url()).searchParams.get('workspaceSmoke')).toBe('preserved');
    const links = await page.getByRole('link').evaluateAll(anchors => anchors.map(anchor => (anchor as HTMLAnchorElement).href));
    const launch = links.find(href => {
      let url = new URL(href);
      if (url.pathname === '/sso/start') url = new URL(url.searchParams.get('next')!, url.origin);
      return url.pathname === '/admin/launch' && url.searchParams.get('companyId') === state.fixture.companyId;
    });
    expect(launch).toBeTruthy();
    await page.goto(launch!, { timeout: 120_000 });
    await page.waitForURL(url => url.origin === origin && ['/admin/choose-work-role', '/holdings/simulation/administration'].includes(url.pathname), { timeout: 90_000 });
    if (new URL(page.url()).pathname === '/admin/choose-work-role') {
      const existingOwner = before.assignments.filter(assignment => assignment.role === 'holding_owner' && assignment.revoked_at === null);
      expect(existingOwner).toHaveLength(1);
      const choice = page.locator('form').filter({ has: page.locator(`input[name="assignmentId"][value="${existingOwner[0].id}"]`) });
      const selected = page.waitForResponse(response => response.request().method() === 'POST' && new URL(response.url()).pathname === '/admin/launch');
      await choice.getByRole('button', { name: 'Use this role', exact: true }).click();
      const selection = await selected;
      expect(selection.status()).toBe(303);
      expect((await selection.request().allHeaders()).origin).toBe(origin);
    }
    await expect(page.getByRole('heading', { name: 'Administration', exact: true })).toBeVisible({ timeout: 90_000 });
    const brand = await page.goto(origin + '/holdings/simulation/administration/brand/identity', { timeout: 120_000 });
    expect(brand?.status()).toBe(200);
    expect(brand?.headers()['x-erpx-app']).toBe('admin');
    const trigger = page.locator('#workspace-content').locator('summary').filter({ hasText: /^Create identity draft$/ });
    await trigger.click();
    const drawer = page.getByRole('dialog', { name: 'Create identity draft', exact: true });
    await expect(drawer).toBeVisible();
    const box = await drawer.boundingBox(), viewport = page.viewportSize()!;
    expect(box).not.toBeNull();
    expect(Math.abs(box!.x + box!.width - viewport.width)).toBeLessThan(2);
    expect(box!.x).toBeGreaterThan(viewport.width * 0.2);
    await info.attach('Admin brand drawer without submission', { body: await page.screenshot(), contentType: 'image/png' });
    await drawer.press('Escape');
    await expect(drawer).toBeHidden();
    const refreshed = await page.reload();
    expect(refreshed?.status()).toBe(200);
    expect(refreshed?.headers()['x-erpx-app']).toBe('admin');
    await expect(trigger).toBeVisible();
    const cookies = (await context.cookies()).filter(cookie => ['__Host-erpx-product', '__Secure-erpx-account.session_token'].includes(cookie.name));
    expect(cookies.some(cookie => cookie.domain === 'admin-dev-erpx.jesadakorn.com')).toBe(true);
    expect(cookies.some(cookie => cookie.domain === 'account-dev-erpx.jesadakorn.com')).toBe(true);
    for (const cookie of cookies) expect({ parentDomain: cookie.domain.startsWith('.'), secure: cookie.secure, httpOnly: cookie.httpOnly }).toEqual({ parentDomain: false, secure: true, httpOnly: true });
    expect(await authority()).toEqual(before);
    expect(errors).toEqual([]);
  } finally { await db.end(); }
});
