import { beforeEach, expect, it, vi } from 'vitest';
import { isValidElement, type ReactElement, type ReactNode } from 'react';
import { isRedirectError } from 'next/dist/client/components/redirect-error';
import { getURLFromRedirectError } from 'next/dist/client/components/redirect';
import { accountSelfLoginHref, accountSelfPageReturn, productEntrySsoHref, type EntryPageSearchParams } from '@erpx/server-identity_access/public/rules/page-entry-return';

vi.mock('server-only', () => ({}));
const state = vi.hoisted(() => ({ host: 'account-dev-erpx.jesadakorn.com' }));
const reads = vi.hoisted(() => ({ session: vi.fn(), centralSession: vi.fn(), choices: vi.fn(), preferences: vi.fn(), access: vi.fn(), security: vi.fn(), profile: vi.fn(), products: vi.fn(), existing: vi.fn(), admin: vi.fn(), provider: vi.fn() }));
vi.mock('next/headers', () => ({ headers: async () => new Headers({ host: state.host }), cookies: async () => ({ get: () => undefined }) }));
vi.mock("@erpx/feature-runtime/lib/product-hosts", () => ({
  requestProductOrigin:(key:string)=>`https://${key}-dev-erpx.jesadakorn.com`,configuredProductOrigin: (key: string) => `https://${key}-dev-erpx.jesadakorn.com`,
  configuredProductHandoff: (key: string) => `https://${key}-dev-erpx.jesadakorn.com/`,
  productHostForHost: (host: string | null) => ['apps', 'account', 'admin', 'finance', 'portal', 'legal-portal', 'forms', 'booking'].find(key => host === `${key}-dev-erpx.jesadakorn.com`) ?? null,
}));
vi.mock("@erpx/server-identity_access/public/authorization", () => ({ currentSession: reads.session }));
vi.mock("@erpx/server-identity_access/public/auth", () => ({ createAuth: async () => ({ api: { getSession: reads.centralSession } }) }));
vi.mock("@erpx/server-identity_access/public/account-self-access", () => ({ currentAccountSelfAccess: reads.access }));
vi.mock("@erpx/server-identity_access/public/account-security", () => ({ currentAccountSecurity: reads.security }));
vi.mock("@erpx/server-identity_access/public/account-profile", () => ({ currentAccountProfile: reads.profile }));
vi.mock("@erpx/server-organization/public/workspace-choices", () => ({ workspaceChoices: reads.choices }));
vi.mock("@erpx/server-work_projection/public/quick-access-preferences", () => ({ readQuickAccessPreferences: reads.preferences }));
vi.mock("@erpx/server-work_projection/public/product-launcher", () => ({ productLaunchChoices: reads.products, existingWorkspaceLaunchChoices: reads.existing }));
vi.mock("@erpx/server-work_projection/public/admin-launcher", () => ({ adminLaunchChoices: reads.admin }));
vi.mock("@erpx/server-work_projection/public/workspace-entry-presentation-query", () => ({ savedAppsWorkspaceForPresentation: async () => null }));
vi.mock("@erpx/server-identity_access/public/provider-entry", () => ({ providerEntryOverview: reads.provider }));
vi.mock("@erpx/server-identity_access/public/account-preference-snapshot", () => ({ preferredAccountLanguage: async () => 'en' }));
vi.mock("@erpx/server-identity_access/public/directory-lookup", () => ({ routedConnection: vi.fn() }));
vi.mock("@erpx/server-runtime/public/presentation-configuration", () => ({ publicBetaEnabled: async () => false }));
vi.mock("@erpx/feature-identity_access/lib/auth-client", () => ({ authClient: { signIn: { email: vi.fn() } } }));

const origin = 'https://account-dev-erpx.jesadakorn.com';
const query = (): EntryPageSearchParams => ({ widget: 'account-roles', tag: ['one', 'two'], companyId: 'acme-co', role: ['employee', 'reviewer'], from: '/holdings/acme/home?view=mine', code: 'private-code', state: 'private-state', token: 'private-token', session_token: 'private-session', _rsc: 'private-rsc' });
const deniedReads = [reads.choices, reads.preferences, reads.access, reads.security, reads.profile, reads.products, reads.existing, reads.admin, reads.provider];
beforeEach(() => {
  vi.resetAllMocks(); state.host = 'account-dev-erpx.jesadakorn.com';
  reads.session.mockRejectedValue(new Error('UNAUTHENTICATED'));
  reads.centralSession.mockResolvedValue(null);
  for (const read of deniedReads) read.mockImplementation(() => { throw new Error('UNAUTHORIZED_DISCOVERY_OR_SELECTION'); });
});

async function redirectFrom(work: () => Promise<ReactNode>) {
  try { await work(); } catch (error) {
    if (isRedirectError(error)) return getURLFromRedirectError(error);
    throw error;
  }
  throw new Error('The actual Server page did not redirect.');
}
function assertQuery(target: string, path: string, widget = 'account-roles') {
  const url = new URL(target, origin);
  expect(url.pathname).toBe(path);
  expect(url.searchParams.get('widget')).toBe(widget);
  expect(url.searchParams.getAll('tag')).toEqual(['one', 'two']);
  expect(url.searchParams.get('companyId')).toBe('acme-co');
  expect(url.searchParams.getAll('role')).toEqual(['employee', 'reviewer']);
  expect(url.searchParams.get('from')).toBe('/holdings/acme/home?view=mine');
  for (const key of ['code', 'state', 'token', 'session_token', '_rsc']) expect(url.searchParams.has(key)).toBe(false);
}
function findLogin(node: ReactNode): ReactElement<{ next?: string }> | undefined {
  if (Array.isArray(node)) { for (const child of node) { const found = findLogin(child); if (found) return found; } return; }
  if (!isValidElement<{ children?: ReactNode }>(node)) return;
  if (typeof node.type === 'function' && node.type.name === 'LoginForm') return node as ReactElement<{ next?: string }>;
  return findLogin(node.props.children);
}

it('keeps ordinary duplicate fields and real Account widget targets while applying only the existing secret sanitizer', () => {
  for (const path of ['/my-access', '/security', '/my-profile'] as const) {
    assertQuery(accountSelfPageReturn(path, query()), path);
    const login = new URL(accountSelfLoginHref(path, query()), origin);
    expect(login.pathname).toBe('/login');
    assertQuery(login.searchParams.get('next')!, path);
  }
});
it('uses the existing native SSO return sanitizer for root query data without choosing Company or role authority', () => {
  const start = new URL(productEntrySsoHref(query()), 'https://apps-dev-erpx.jesadakorn.com');
  expect(start.pathname).toBe('/sso/start');
  assertQuery(start.searchParams.get('next')!, '/');
  for (const read of deniedReads) expect(read).not.toHaveBeenCalled();
});
it('retains original no-query defaults and fails closed for control characters or overlong returns', () => {
  expect(accountSelfLoginHref('/my-access')).toBe('/login?next=/my-access');
  expect(accountSelfLoginHref('/security')).toBe('/login?next=/security');
  expect(accountSelfLoginHref('/my-profile')).toBe('/login?next=%2Fmy-profile');
  expect(productEntrySsoHref()).toBe('/sso/start');
  expect(productEntrySsoHref({ code: 'secret', state: 'secret', token: 'secret', session_token: 'secret', _rsc: 'secret' })).toBe('/sso/start');
  for (const value of ['\u0000', '\\', 'x'.repeat(5000)]) {
    expect(accountSelfPageReturn('/security', { widget: value })).toBe('/security');
    expect(productEntrySsoHref({ widget: value })).toBe('/sso/start');
  }
});
it.each(['/my-access', '/security', '/my-profile'] as const)('preserves the actual unauthenticated %s Server-page return and blocks private reads', async path => {
  const page = path === '/my-access' ? (await import('@erpx/feature-account/routes/my-access/page')).default : path === '/security' ? (await import('@erpx/feature-account/routes/security/page')).default : (await import('@erpx/feature-account/routes/my-profile/page')).default;
  const redirect = new URL(await redirectFrom(() => page({ searchParams: Promise.resolve(query()) })), origin);
  expect(redirect.pathname).toBe('/login');
  assertQuery(redirect.searchParams.get('next')!, path);
  expect(reads.session).toHaveBeenCalledTimes(1);
  for (const read of deniedReads) expect(read).not.toHaveBeenCalled();
});
it.each(['apps', 'admin', 'finance'])('preserves %s root query through its actual Server-page SSO redirect without selecting a role', async host => {
  state.host = `${host}-dev-erpx.jesadakorn.com`;
  const { default: Page } = await import('@erpx/workplace/routes/product-entry/page');
  const redirect = new URL(await redirectFrom(() => Page({ searchParams: Promise.resolve(query()) })), `https://${state.host}`);
  expect(redirect.pathname).toBe('/sso/start');
  assertQuery(redirect.searchParams.get('next')!, '/');
  expect(reads.centralSession).toHaveBeenCalledTimes(1);
  for (const read of deniedReads) expect(read).not.toHaveBeenCalled();
});
it('retains the exact legacy self-profile query in its existing configured Account-host redirect before authentication', async () => {
  state.host = 'console-dev-erpx.jesadakorn.com';
  const { default: Page } = await import('@erpx/feature-account/routes/my-profile/page');
  const target = new URL(await redirectFrom(() => Page({ searchParams: Promise.resolve(query()) })));
  expect(target.origin).toBe(origin); assertQuery(target.href, '/my-profile');
  expect(reads.session).not.toHaveBeenCalled();
});
it('passes the preserved real widget target through the actual central Login Server page and its existing closed Account bridge', async () => {
  const { default: Page } = await import('@erpx/feature-account/routes/security/page');
  const redirect = new URL(await redirectFrom(() => Page({ searchParams: Promise.resolve({ ...query(), widget: 'account-security-controls' }) })), origin);
  const { default: Login } = await import('@erpx/workplace/routes/login/page');
  const node = findLogin(await Login({ searchParams: Promise.resolve({ next: redirect.searchParams.get('next')! }) }));
  expect(node).toBeDefined(); if (!node?.props.next) throw new Error('The actual Login page did not preserve its closed return.');
  const bridge = new URL(node.props.next, origin);
  expect(bridge.pathname).toBe('/account/launch');
  assertQuery(bridge.searchParams.get('next')!, '/security', 'account-security-controls');
});

function component(node: ReactNode, name: string): ReactElement<Record<string, unknown>> | undefined {
  if (Array.isArray(node)) { for (const child of node) { const result = component(child, name); if (result) return result; } return; }
  if (!isValidElement<{ children?: ReactNode }>(node)) return;
  if (typeof node.type === 'function' && node.type.name === name) return node as ReactElement<Record<string, unknown>>;
  return component(node.props.children, name);
}
it('keeps Feed home and includes other-product and Admin-only discovery in its Navbar modal', async () => {
  state.host = 'apps-dev-erpx.jesadakorn.com';
  reads.centralSession.mockResolvedValue({ user: { id: 'feed-account', name: 'Feed user' } });
  const scope = { kind: 'holding', holdingSlug: 'group' };
  const common = { companyId: 'company-one', company: 'Company One', holding: 'Group', workspace: 'Company One', scope, purchasePurpose: 'new-work' };
  const feed = { ...common, productId: 'apps', href: 'https://apps-dev-erpx.jesadakorn.com/sso/start?next=authorized-feed' };
  const finance = { ...common, productId: 'finance', href: 'https://finance-dev-erpx.jesadakorn.com/sso/start?next=authorized-finance' };
  const admin = { ...common, kind: 'company', href: 'https://admin-dev-erpx.jesadakorn.com/sso/start?next=authorized-admin' };
  reads.products.mockResolvedValue([feed, finance]); reads.admin.mockResolvedValue([admin]); reads.existing.mockResolvedValue([]); reads.provider.mockResolvedValue(null);
  const { default: Page } = await import('@erpx/workplace/entries/workspace');
  const root = component(await Page({}), 'AppsLauncher');
  expect(root?.props).toMatchObject({ accountId: 'feed-account', entry: true, companies: [{ workspaces: [{ adminHref: admin.href, apps: [{ productId: 'apps', label: 'Feed', href: feed.href }, { productId: 'finance' }] }] }] });
  expect(await redirectFrom(() => Page({ applications: true }))).toBe('/?launcher=open');
  reads.products.mockResolvedValue([]);
  expect(component(await Page({}), 'AppsLauncher')?.props.companies).toMatchObject([{ workspaces: [{ apps: [], adminHref: admin.href }] }]);
});
it('redirects an old applications bookmark to the Apps modal before discovery', async () => {
  state.host = 'apps-dev-erpx.jesadakorn.com';
  const { default: Page } = await import('@erpx/workplace/entries/workspace');
  expect(await redirectFrom(() => Page({ applications: true }))).toBe('/?launcher=open');
  for (const read of deniedReads) expect(read).not.toHaveBeenCalled();
});
it.each(['apps', 'admin', 'finance'])('isolates the new Apps selector from the actual %s shared Server entry', async host => {
  state.host = `${host}-dev-erpx.jesadakorn.com`;
  reads.centralSession.mockResolvedValue({ user: { id: 'account-one', name: 'One' } });
  const choice = { companyId: 'company-one', company: 'Company One', holding: 'Group', workspace: 'Company One', scope: { kind: 'holding', holdingSlug: 'group' }, productId: 'finance', href: 'https://finance-dev-erpx.jesadakorn.com/sso/start?next=current', purchasePurpose: 'new-work' };
  reads.products.mockResolvedValue([choice]); reads.admin.mockResolvedValue([]); reads.existing.mockResolvedValue([]); reads.provider.mockResolvedValue(null);
  const { default: Page } = await import('@erpx/workplace/entries/workspace');
  if (host === 'finance') {
    expect(await redirectFrom(() => Page({}))).toBe(choice.href);
    expect(reads.products).toHaveBeenCalledWith('finance');
    expect(reads.admin).not.toHaveBeenCalled(); return;
  }
  const node = await Page({});
  const selector = component(node, 'AppsLauncher');
  if (host === 'apps') {
    expect(selector?.props.accountId).toBe('account-one');
    expect(selector?.props.companies).toMatchObject([{ label: 'Company One', workspaces: [{ apps: [{ productId: 'finance', href: choice.href }] }] }]);
    expect(component(node, 'AppsSidebar')).toBeUndefined();
  } else {
    expect(selector).toBeUndefined(); expect(component(node, 'AppsSidebar')).toBeDefined();
  }
});
