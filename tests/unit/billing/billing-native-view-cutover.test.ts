import { readFile } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const src = (name: string) =>
  readFile(new URL(`../../../packages/server-billing/src/${name}`, import.meta.url), 'utf8');

describe('billing native-authoritative view cutover', () => {
  it('removes the TypeScript view SQL and export from the owner adapter and barrel', async () => {
    const billing = await src('adapters/billing.ts');
    expect(billing).not.toContain('nativeBillingView');
    expect(billing).not.toContain('export const billingView');
    // The three retired literals (generic table read, plans IN-list, stripe_inbox join) are gone.
    expect(billing).not.toContain('SELECT * FROM ${table} WHERE holding_id=$1 ${order}');
    expect(billing).not.toContain("SELECT * FROM billing_plans WHERE id IN ('infrastructure','standard','priority')");
    expect(billing).not.toContain('SELECT i.id,\'attention\' AS state,i.last_error FROM stripe_inbox');
    // Advisory locks and FOR UPDATE statements stay untouched.
    expect(billing).toContain('BILLING_ADVISORY_XACT_LOCK_SQL');
    expect(billing).toContain('"SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE"');
    expect(billing).toContain('"SELECT * FROM billing_accounts WHERE holding_id=$1 FOR UPDATE"');

    const barrel = await src('public/billing.ts');
    expect(barrel).not.toContain('billingView');
  });

  it('makes the platform presentation a sole native reader that fails closed', async () => {
    const presentation = await src('adapters/billing-presentation.ts');
    expect(presentation).not.toContain('billingView');
    expect(presentation).not.toContain('nativeBillingAvailable');
    expect(presentation).toContain('nativeBillingWorkspace');
    expect(presentation).toContain("if (!data)");
    expect(presentation).toContain("NATIVE_BILLING_UNAVAILABLE");
  });

  it('binds the API view port to the native workspace with no TypeScript fallback', async () => {
    const api = await src('adapters/billing-api.ts');
    expect(api).not.toContain('billingView');
    expect(api).toContain('nativeBillingWorkspace');
    expect(api).toContain('view:(actor,id)=>nativeBillingWorkspace(');
    const application = await src('application/api.ts');
    expect(application).toContain('const data = await ports.view(actor, holdingId)');
    expect(application).toContain('NATIVE_BILLING_UNAVAILABLE');
  });

  it('fails the GET view closed when the native projection is unavailable', async () => {
    const { createBillingApi } = await import('@erpx/server-billing/application/api');
    const ports = {
      ready: async () => {},
      repository: {
        platformAccounts: async () => ({ rows: [] }),
        holdingSlug: async () => ({ rows: [{ slug: 'acme' }] }),
      },
      platformActor: async () => ({ userId: 'admin', email: 'admin@test', role: 'platform_admin' }),
      error: (status: number, code: string, message: string) =>
        Object.assign(new Error(message), { status, code }),
      errorResponse: (error: { status?: number; code?: string }) =>
        Response.json({ code: error.code }, { status: error.status ?? 500 }),
      view: async () => undefined,
    };
    const run = createBillingApi(ports as never).billingHandler(true);
    const response = await run(
      new Request('https://fixture.example/api?holdingId=h001'),
      { params: Promise.resolve({ path: [] }) },
    );
    expect(response.status).toBe(503);
    expect(await response.json()).toEqual({ code: 'NATIVE_BILLING_UNAVAILABLE' });
  });

  it('returns the native projection and forwards the resolved actor when configured', async () => {
    const { createBillingApi } = await import('@erpx/server-billing/application/api');
    const projection = { sandbox: true, now: 1, account: { holding_id: 'h001' } };
    let received: { userId: string; holdingId: string } | undefined;
    const ports = {
      ready: async () => {},
      repository: {
        platformAccounts: async () => ({ rows: [] }),
        holdingSlug: async () => ({ rows: [{ slug: 'acme' }] }),
      },
      platformActor: async () => ({ userId: 'admin', email: 'admin@test', role: 'platform_admin' }),
      error: (status: number, code: string, message: string) =>
        Object.assign(new Error(message), { status, code }),
      errorResponse: (error: { status?: number; code?: string }) =>
        Response.json({ code: error.code }, { status: error.status ?? 500 }),
      view: async (actor: { userId: string }, holdingId: string) => {
        received = { userId: actor.userId, holdingId };
        return projection;
      },
    };
    const run = createBillingApi(ports as never).billingHandler(true);
    const response = await run(
      new Request('https://fixture.example/api?holdingId=h001'),
      { params: Promise.resolve({ path: [] }) },
    );
    expect(response.status).toBe(200);
    expect((await response.json()).data).toEqual(projection);
    expect(received).toEqual({ userId: 'admin', holdingId: 'h001' });
  });
});
