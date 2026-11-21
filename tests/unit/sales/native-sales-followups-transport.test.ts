import { afterEach, describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  NativeFollowupError,
  createNativeFollowupClient,
  customerFollowupChangeInput,
  customerFollowupCreateInput,
  nativeFollowupFailure,
  type NativeFollowupPageContext,
} from '../../../packages/feature-sales/src/public/native-sales-followups';

const incoming = (overrides: Record<string, string> = {}) =>
  new Headers({ host: 'acme.example.com', cookie: '__Host-erpx-product=session', ...overrides });
const actor: NativeFollowupPageContext = {
  userId: 'manager',
  holdingId: 'h001',
  holdingSlug: 'acme',
  companyId: 'company',
  branchId: null,
  departmentId: null,
  assignmentId: 'assignment-manager',
  role: 'sales_manager',
  assignmentScope: 'holding',
  authority: { revision: 7, assignmentVersion: 3 },
  modules: ['sales'],
  iam: { permissions: ['commerce.read', 'commerce.sales', 'commerce.approve'] },
};
const context = {
  userId: 'manager',
  holdingId: 'h001',
  companyId: 'company',
  branchId: null,
  departmentId: null,
  branchSlug: null,
  departmentSlug: null,
  assignmentId: 'assignment-manager',
  assignmentVersion: 3,
  revision: 7,
  role: 'sales_manager',
  assignmentScopeKind: 'holding',
  modules: ['sales'],
  permissions: ['commerce.read', 'commerce.sales', 'commerce.approve'],
  expiresAt: 9_007_199_254_740_991,
};
const create = {
  orderId: 'order-1',
  expectedVersion: 1,
  ownerId: 'owner',
  dueDate: '2026-09-22',
  nextAction: 'Call the customer about installation requirements',
};
const change = {
  action: 'reassign' as const,
  expectedVersion: 1,
  ownerId: 'manager',
  dueDate: '2026-09-23',
  reason: 'Retain the original customer promise under a new owner',
};
const ok = (data: unknown) =>
  new Response(JSON.stringify({ data, context }), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });

afterEach(() => vi.unstubAllEnvs());

describe('native Sales customer follow-up transport', () => {
  it('forwards create and change to the exact native paths', async () => {
    const transport = vi
      .fn()
      .mockResolvedValueOnce(ok({ id: 'followup-key-0001', version: 1, status: 'open' }))
      .mockResolvedValueOnce(ok({ id: 'followup-1', version: 2, status: 'open' }));
    const client = createNativeFollowupClient(
      {
        privateBaseUrl: 'http://127.0.0.1:44994',
        timeoutMs: 1000,
        holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
      },
      transport,
    );
    expect(
      await client.create(incoming(), actor, create, 'followup-key-0001'),
    ).toEqual({ id: 'followup-key-0001', version: 1, status: 'open' });
    const [method, url, headers, body] = transport.mock.calls[0];
    expect(method).toBe('POST');
    expect(url.toString()).toBe(
      'http://127.0.0.1:44994/api/v1/holdings/acme/customer-followups',
    );
    expect(headers.get('idempotency-key')).toBe('followup-key-0001');
    expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
    expect(JSON.parse(new TextDecoder().decode(body))).toEqual(create);

    await client.change(incoming(), actor, 'followup-1', change, 'followup-key-0002');
    expect(transport.mock.calls[1][1].toString()).toBe(
      'http://127.0.0.1:44994/api/v1/holdings/acme/customer-followups/followup-1',
    );
  });

  it('strictly validates create and change bodies', () => {
    expect(customerFollowupCreateInput(create)).toEqual(create);
    expect(customerFollowupChangeInput(change)).toEqual(change);
    expect(customerFollowupChangeInput({ action: 'complete', expectedVersion: 2, reason: 'Recorded outcome' })).toEqual({
      action: 'complete',
      expectedVersion: 2,
      reason: 'Recorded outcome',
    });
    for (const invalid of [
      null,
      { ...create, expectedVersion: 2 },
      { ...create, dueDate: '2026-02-30' },
      { ...create, nextAction: 'x' },
      { ...create, extra: true },
    ]) {
      let error: unknown;
      try {
        customerFollowupCreateInput(invalid);
      } catch (caught) {
        error = caught;
      }
      expect(error).toBeInstanceOf(NativeFollowupError);
      expect((error as NativeFollowupError).code).toBe('CUSTOMER_FOLLOWUP_REQUEST_INVALID');
    }
    for (const invalid of [
      { action: 'complete', expectedVersion: 1, reason: 'Recorded outcome', ownerId: 'x' },
      { action: 'reassign', expectedVersion: 1, reason: 'Recorded outcome' },
      { action: 'unknown', expectedVersion: 1, reason: 'Recorded outcome' },
    ]) {
      let error: unknown;
      try {
        customerFollowupChangeInput(invalid);
      } catch (caught) {
        error = caught;
      }
      expect(error).toBeInstanceOf(NativeFollowupError);
    }
  });

  it('fails closed on foreign hosts, context drift, invalid receipts and owner errors', async () => {
    const client = (response: Response) =>
      createNativeFollowupClient(
        {
          privateBaseUrl: 'http://127.0.0.1:44994',
          timeoutMs: 1000,
          holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
        },
        vi.fn().mockResolvedValue(response),
      );
    await expect(
      client(ok({ id: 'followup-1', version: 1, status: 'open' })).create(
        incoming({ host: 'other.example.com' }),
        actor,
        create,
        'followup-key-0001',
      ),
    ).rejects.toMatchObject({ code: 'NATIVE_SALES_FOLLOWUPS_AUTHORITY_INVALID' });
    await expect(
      client(
        new Response(JSON.stringify({ data: { id: 'followup-1', version: 1, status: 'open' }, context: { ...context, revision: 8 } }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        }),
      ).create(incoming(), actor, create, 'followup-key-0001'),
    ).rejects.toMatchObject({ code: 'NATIVE_SALES_FOLLOWUPS_CONTEXT_CHANGED' });
    await expect(
      client(ok({ id: 'followup-1', version: 0, status: 'open' })).create(
        incoming(),
        actor,
        create,
        'followup-key-0001',
      ),
    ).rejects.toMatchObject({ code: 'NATIVE_SALES_FOLLOWUPS_RESPONSE_INVALID' });
    await expect(
      client(
        new Response(JSON.stringify({ error: { code: 'CUSTOMER_FOLLOWUP_OWNER_REQUIRED', message: 'Synthetic owner error.' } }), {
          status: 403,
          headers: { 'content-type': 'application/json' },
        }),
      ).change(incoming(), actor, 'followup-1', change, 'followup-key-0002'),
    ).rejects.toMatchObject({ code: 'CUSTOMER_FOLLOWUP_OWNER_REQUIRED', status: 403 });
  });

  it('maps owner codes without collapsing conflict truth', () => {
    expect(
      nativeFollowupFailure(new NativeFollowupError('CONFLICT', 409)),
    ).toMatchObject({ status: 409 });
    expect(
      nativeFollowupFailure(new NativeFollowupError('NATIVE_SALES_FOLLOWUPS_UNAVAILABLE', 502)),
    ).toMatchObject({ status: 503 });
    expect(
      nativeFollowupFailure(new NativeFollowupError('NATIVE_SALES_FOLLOWUPS_CONTEXT_CHANGED')),
    ).toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
    expect(nativeFollowupFailure(new Error('unrelated'))).toBeUndefined();
  });
});
