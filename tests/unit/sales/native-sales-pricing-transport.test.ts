import { describe, it, expect, vi, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  createNativeSalesPricingClient,
  nativeSalesPricingAvailable,
  nativeSalesPricingCommand,
  nativeSalesPricingConfiguration,
  nativeSalesPricingFailure,
  NativeSalesPricingError,
  type NativeSalesPricingPageContext,
} from '../../../packages/feature-sales/src/public/native-sales-pricing';

const config = (overrides: Record<string, unknown> = {}) =>
  JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44994',
    timeoutMs: 1000,
    holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
    ...overrides,
  });
const incoming = (overrides: Record<string, string> = {}) =>
  new Headers({ host: 'acme.example.com', cookie: '__Host-erpx-product=session', ...overrides });
const actor: NativeSalesPricingPageContext = {
  userId: 'user',
  holdingId: 'h001',
  holdingSlug: 'acme',
  companyId: 'company',
  branchId: null,
  departmentId: null,
  assignmentId: 'assignment',
  role: 'owner',
  assignmentScope: 'holding',
  authority: { revision: 7, assignmentVersion: 3 },
  modules: ['sales'],
  iam: { permissions: ['commerce.read', 'commerce.sales', 'commerce.approve'] },
};
const context = {
  userId: 'user',
  holdingId: 'h001',
  companyId: 'company',
  branchId: null,
  departmentId: null,
  branchSlug: null,
  departmentSlug: null,
  assignmentId: 'assignment',
  assignmentVersion: 3,
  revision: 7,
  role: 'owner',
  assignmentScopeKind: 'holding',
  modules: ['sales'],
  permissions: ['commerce.read', 'commerce.sales', 'commerce.approve'],
  expiresAt: 9_007_199_254_740_991,
};
const receipt = { id: 'price-list-1', status: 'draft', version: 1 };
const ok = (data: unknown = receipt, supplied = context) =>
  new Response(JSON.stringify({ data, context: supplied }), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });

afterEach(() => vi.unstubAllEnvs());

describe('native Sales pricing transport', () => {
  it('forwards the configured Holding, session, CAS and create body', async () => {
    const transport = vi.fn().mockResolvedValue(ok());
    const client = createNativeSalesPricingClient(nativeSalesPricingConfiguration(config()), transport);
    const input = {
      name: 'Standard pricing',
      currency: 'THB',
      customerId: 'customer',
      validFrom: '2026-11-01',
      validUntil: '2026-11-30',
      maxAutoDiscountBps: 500,
      items: [{ productId: 'product-1', price: '100.50', taxBps: 700 }],
    };
    const result = await client.command(incoming(), actor, 'create', null, input, 'price-list-http-0001');
    expect(result).toEqual(receipt);
    const [method, url, headers, body] = transport.mock.calls[0];
    expect(method).toBe('POST');
    expect(url.toString()).toBe('http://127.0.0.1:44994/api/v1/holdings/acme/sales/pricing');
    expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
    expect(headers.get('content-type')).toBe('application/json');
    expect(headers.get('idempotency-key')).toBe('price-list-http-0001');
    expect(headers.get('x-erpx-context-company')).toBe('company');
    expect(headers.get('x-erpx-context-assignment')).toBe('assignment');
    expect(headers.get('x-erpx-context-revision')).toBe('7');
    expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
    expect(new TextDecoder().decode(body)).toBe(JSON.stringify(input));
  });

  it('uses exact edit and decision paths and preserves the command key', async () => {
    const transport = vi.fn().mockImplementation(async () => ok({ ...receipt, version: 2 }));
    const client = createNativeSalesPricingClient(nativeSalesPricingConfiguration(config()), transport);
    await client.command(incoming(), actor, 'update', 'price-list-1', { expectedVersion: 1 }, 'price-list-http-0002');
    await client.command(
      incoming(),
      actor,
      'decide',
      'price-list-1',
      { expectedVersion: 2, action: 'activate', reason: 'Independent review' },
      'price-list-http-0003',
    );
    const [, updateUrl] = transport.mock.calls[0];
    const [, decideUrl] = transport.mock.calls[1];
    expect(updateUrl.toString()).toBe(
      'http://127.0.0.1:44994/api/v1/holdings/acme/sales/pricing/price-list-1/edit',
    );
    expect(decideUrl.toString()).toBe(
      'http://127.0.0.1:44994/api/v1/holdings/acme/sales/pricing/price-list-1',
    );
  });

  it('rejects scope, host, key and oversized body before transport', async () => {
    const client = createNativeSalesPricingClient(nativeSalesPricingConfiguration(config()), vi.fn().mockResolvedValue(ok()));
    await expect(
      client.command(incoming({ host: 'evil.example.com' }), actor, 'create', null, {}, 'price-list-http-0004'),
    ).rejects.toMatchObject({ code: 'NATIVE_SALES_PRICING_AUTHORITY_INVALID' });
    await expect(
      client.command(incoming(), { ...actor, branchId: 'branch' }, 'create', null, {}, 'price-list-http-0005'),
    ).rejects.toMatchObject({ code: 'COMPANY_PRICING_SCOPE_REQUIRED', status: 403 });
    await expect(
      client.command(incoming(), actor, 'create', null, {}, 'short'),
    ).rejects.toMatchObject({ code: 'NATIVE_SALES_PRICING_REQUEST_INVALID' });
    await expect(
      client.command(incoming(), actor, 'update', 'bad/id', {}, 'price-list-http-0006'),
    ).rejects.toMatchObject({ code: 'NATIVE_SALES_PRICING_REQUEST_INVALID' });
    await expect(
      client.command(incoming(), actor, 'create', null, 'x'.repeat(256 * 1024), 'price-list-http-0007'),
    ).rejects.toMatchObject({ code: 'NATIVE_SALES_PRICING_REQUEST_INVALID' });
  });

  it('rejects partial, foreign, changed-context and malformed receipts', async () => {
    const clientFor = (value: unknown) =>
      createNativeSalesPricingClient(
        nativeSalesPricingConfiguration(config()),
        vi.fn().mockResolvedValue(
          new Response(JSON.stringify(value), {
            status: 200,
            headers: { 'content-type': 'application/json' },
          }),
        ),
      );
    await expect(
      clientFor({ data: { ...receipt, extra: true }, context }).command(
        incoming(), actor, 'create', null, {}, 'price-list-http-0008',
      ),
    ).rejects.toMatchObject({ code: 'NATIVE_SALES_PRICING_RESPONSE_INVALID' });
    await expect(
      clientFor({ data: receipt, context: { ...context, companyId: 'foreign' } }).command(
        incoming(), actor, 'create', null, {}, 'price-list-http-0009',
      ),
    ).rejects.toMatchObject({ code: 'NATIVE_SALES_PRICING_CONTEXT_CHANGED' });
    await expect(
      clientFor({ data: receipt, context: { ...context, revision: 8 } }).command(
        incoming(), actor, 'create', null, {}, 'price-list-http-0010',
      ),
    ).rejects.toMatchObject({ code: 'NATIVE_SALES_PRICING_CONTEXT_CHANGED' });
    await expect(
      clientFor({ data: { ...receipt, status: 'paid' }, context }).command(
        incoming(), actor, 'create', null, {}, 'price-list-http-0011',
      ),
    ).rejects.toMatchObject({ code: 'NATIVE_SALES_PRICING_RESPONSE_INVALID' });
    await expect(
      clientFor({ data: { ...receipt, version: 0 }, context }).command(
        incoming(), actor, 'create', null, {}, 'price-list-http-0012',
      ),
    ).rejects.toMatchObject({ code: 'NATIVE_SALES_PRICING_RESPONSE_INVALID' });
  });

  it('validates the exact loopback/Holding configuration', () => {
    expect(nativeSalesPricingConfiguration(config()).holdings).toHaveLength(1);
    for (const bad of [
      config({ privateBaseUrl: 'http://localhost:44994' }),
      config({ privateBaseUrl: 'http://127.0.0.1:3100' }),
      config({ privateBaseUrl: 'http://127.0.0.1:3210' }),
      config({ privateBaseUrl: 'http://127.0.0.1:44994/path' }),
      config({ timeoutMs: 249 }),
      config({ timeoutMs: 30001 }),
      config({ holdings: [] }),
      config({ holdings: [{ origin: 'http://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }] }),
      config({ holdings: [{ origin: 'https://acme.example.com/path', holdingSlug: 'acme', holdingId: 'h001' }] }),
      config({
        holdings: [
          { origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' },
          { origin: 'https://acme.example.com', holdingSlug: 'other', holdingId: 'h002' },
        ],
      }),
    ])
      expect(() => createNativeSalesPricingClient(nativeSalesPricingConfiguration(bad))).toThrow(NativeSalesPricingError);
  });

  it('maps owner failures and fails closed without falling back', () => {
    expect(nativeSalesPricingFailure(new NativeSalesPricingError('PRICE_LIST_CHANGED', 409, 'changed'))).toEqual({
      status: 409,
      code: 'PRICE_LIST_CHANGED',
      message: 'changed',
    });
    expect(nativeSalesPricingFailure(new NativeSalesPricingError('PRICE_LIST_REQUEST_INVALID', 422, 'bad'))).toEqual({
      status: 422,
      code: 'PRICE_LIST_REQUEST_INVALID',
      message: 'bad',
    });
    expect(nativeSalesPricingFailure(new NativeSalesPricingError('NATIVE_SALES_PRICING_UNAVAILABLE', 503))).toEqual({
      status: 503,
      code: 'NATIVE_SALES_PRICING_UNAVAILABLE',
      message: 'The native Sales pricing service is unavailable.',
    });
    expect(nativeSalesPricingFailure(new NativeSalesPricingError('NATIVE_SALES_PRICING_CONTEXT_CHANGED'))).toEqual({
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before retrying.',
    });
    expect(nativeSalesPricingFailure(new NativeSalesPricingError('NATIVE_SALES_PRICING_UNAVAILABLE'))).toEqual({
      status: 503,
      code: 'NATIVE_SALES_PRICING_UNAVAILABLE',
      message: 'The native Sales pricing service is unavailable.',
    });
    expect(nativeSalesPricingFailure(new Error('other'))).toBeUndefined();
  });

  it('keeps the retained owner when the selector is unset and maps a configured failure', async () => {
    delete process.env.ERPX_NATIVE_SALES_PRICING_CONFIG;
    expect(nativeSalesPricingAvailable()).toBe(false);
    expect(
      await nativeSalesPricingCommand(actor as never, 'create', null, {}, 'price-list-http-0013', incoming()),
    ).toBeUndefined();

    vi.stubEnv('ERPX_NATIVE_SALES_PRICING_CONFIG', config());
    expect(nativeSalesPricingAvailable()).toBe(true);
    await expect(
      nativeSalesPricingCommand(
        { ...actor, branchId: 'branch' } as never,
        'create',
        null,
        {},
        'price-list-http-0014',
        incoming(),
      ),
    ).rejects.toMatchObject({ status: 403, code: 'COMPANY_PRICING_SCOPE_REQUIRED' });
  });
});
