import { afterEach, describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  NativeSalesNotificationError,
  createNativeSalesNotificationClient,
  nativeSalesNotificationFailure,
  nativeSalesNotificationReconcile,
  salesNotificationReconciliationInput,
  type NativeSalesNotificationPageContext,
} from '../../../packages/feature-sales/src/public/native-sales-notification-reconciliation';

const incoming = (overrides: Record<string, string> = {}) =>
  new Headers({ host: 'acme.example.com', cookie: '__Host-erpx-product=session', ...overrides });
const actor: NativeSalesNotificationPageContext = {
  userId: 'reviewer',
  holdingId: 'h001',
  holdingSlug: 'acme',
  companyId: 'company',
  branchId: null,
  departmentId: null,
  assignmentId: 'assignment-reviewer',
  role: 'sales_manager',
  assignmentScope: 'holding',
  authority: { revision: 7, assignmentVersion: 3 },
  modules: ['sales'],
  iam: { permissions: ['commerce.read', 'commerce.sales', 'commerce.approve'] },
};
const context = {
  userId: 'reviewer',
  holdingId: 'h001',
  companyId: 'company',
  branchId: null,
  departmentId: null,
  branchSlug: null,
  departmentSlug: null,
  assignmentId: 'assignment-reviewer',
  assignmentVersion: 3,
  revision: 7,
  role: 'sales_manager',
  assignmentScopeKind: 'holding',
  modules: ['sales'],
  permissions: ['commerce.read', 'commerce.sales', 'commerce.approve'],
  expiresAt: 9_007_199_254_740_991,
};
const receipt = {
  id: 'notice-1',
  orderId: 'order-1',
  status: 'sent' as const,
  version: 3,
  reconciliationId: 'sales-notification-reconciliation-key-0001',
};
const input = {
  expectedVersion: 2,
  attemptId: 'attempt-1',
  reason: 'Independent current approver verifies the original provider identity',
};
const ok = (data: unknown = receipt, supplied = context) =>
  new Response(JSON.stringify({ data, context: supplied }), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
const failure = (status: number, code: string) =>
  new Response(JSON.stringify({ error: { code, message: 'Synthetic owner error.' } }), {
    status,
    headers: { 'content-type': 'application/json' },
  });

afterEach(() => vi.unstubAllEnvs());

describe('native Sales customer-notice reconciliation transport', () => {
  it('forwards the exact Holding, session, key, version and attempt', async () => {
    const transport = vi.fn().mockResolvedValue(ok());
    const client = createNativeSalesNotificationClient(
      {
        privateBaseUrl: 'http://127.0.0.1:44994',
        timeoutMs: 1000,
        holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
      },
      transport,
    );
    const result = await client.reconcile(
      incoming(),
      actor,
      'notice-1',
      input,
      'reconcile-key-0001',
    );
    expect(result).toEqual(receipt);
    const [method, url, headers, body] = transport.mock.calls[0];
    expect(method).toBe('POST');
    expect(url.toString()).toBe(
      'http://127.0.0.1:44994/api/v1/holdings/acme/customer-notifications/notice-1/reconcile',
    );
    expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
    expect(headers.get('host')).toBe('acme.example.com');
    expect(headers.get('idempotency-key')).toBe('reconcile-key-0001');
    expect(headers.get('x-erpx-context-company')).toBe('company');
    expect(headers.get('x-erpx-context-assignment')).toBe('assignment-reviewer');
    expect(headers.get('x-erpx-context-revision')).toBe('7');
    expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
    expect(JSON.parse(new TextDecoder().decode(body))).toEqual(input);
  });

  it('strictly validates the reconciliation body before any transport', async () => {
    expect(salesNotificationReconciliationInput(input)).toEqual(input);
    for (const invalid of [
      null,
      {},
      { ...input, expectedVersion: 0 },
      { ...input, expectedVersion: '2' },
      { ...input, attemptId: '' },
      { ...input, attemptId: ' padded ' },
      { ...input, reason: 'tiny' },
      { ...input, reason: 'x'.repeat(1001) },
      { ...input, extra: true },
    ]) {
      let error: unknown;
      try {
        salesNotificationReconciliationInput(invalid);
      } catch (caught) {
        error = caught;
      }
      expect(error).toBeInstanceOf(NativeSalesNotificationError);
      expect((error as NativeSalesNotificationError).code).toBe(
        'SALES_NOTIFICATION_RECONCILIATION_REQUEST_INVALID',
      );
    }
  });

  it('fails closed on context, receipt, redirect and bounded responses', async () => {
    const client = (response: Response | Promise<Response>) =>
      createNativeSalesNotificationClient(
        {
          privateBaseUrl: 'http://127.0.0.1:44994',
          timeoutMs: 1000,
          holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
        },
        vi.fn().mockResolvedValue(response),
      );
    await expect(
      client(ok(receipt, { ...context, revision: 8 })).reconcile(
        incoming(),
        actor,
        'notice-1',
        input,
        'reconcile-key-0001',
      ),
    ).rejects.toMatchObject({ code: 'NATIVE_SALES_NOTIFICATION_RECONCILIATION_CONTEXT_CHANGED' });
    for (const invalid of [
      { ...receipt, status: 'unknown' },
      { ...receipt, version: 1 },
      { ...receipt, reconciliationId: '' },
      { ...receipt, extra: true },
    ])
      await expect(
        client(ok(invalid)).reconcile(incoming(), actor, 'notice-1', input, 'reconcile-key-0001'),
      ).rejects.toMatchObject({
        code: 'NATIVE_SALES_NOTIFICATION_RECONCILIATION_RESPONSE_INVALID',
      });
    await expect(
      client(
        new Response(JSON.stringify({ data: receipt, context }), {
          status: 200,
          headers: { 'content-type': 'text/plain' },
        }),
      ).reconcile(incoming(), actor, 'notice-1', input, 'reconcile-key-0001'),
    ).rejects.toMatchObject({ code: 'NATIVE_SALES_NOTIFICATION_RECONCILIATION_RESPONSE_INVALID' });
    await expect(
      client(failure(409, 'CUSTOMER_NOTIFICATION_PROVIDER_MISMATCH')).reconcile(
        incoming(),
        actor,
        'notice-1',
        input,
        'reconcile-key-0001',
      ),
    ).rejects.toMatchObject({
      code: 'CUSTOMER_NOTIFICATION_PROVIDER_MISMATCH',
      status: 409,
    });
  });

  it('rejects foreign hosts, malformed keys and unconfigured runtime without fallback', async () => {
    const transport = vi.fn();
    const client = createNativeSalesNotificationClient(
      {
        privateBaseUrl: 'http://127.0.0.1:44994',
        timeoutMs: 1000,
        holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
      },
      transport,
    );
    await expect(
      client.reconcile(incoming({ host: 'other.example.com' }), actor, 'notice-1', input, 'reconcile-key-0001'),
    ).rejects.toMatchObject({
      code: 'NATIVE_SALES_NOTIFICATION_RECONCILIATION_AUTHORITY_INVALID',
    });
    await expect(
      client.reconcile(incoming(), actor, 'notice-1', input, 'short'),
    ).rejects.toMatchObject({ code: 'SALES_NOTIFICATION_RECONCILIATION_REQUEST_INVALID' });
    expect(transport).not.toHaveBeenCalled();
    await expect(
      nativeSalesNotificationReconcile(
        { ...actor, userId: actor.userId } as never,
        'notice-1',
        input,
        'reconcile-key-0001',
        incoming(),
      ),
    ).rejects.toMatchObject({ code: 'NATIVE_SALES_NOTIFICATION_RECONCILIATION_UNAVAILABLE' });
  });

  it('maps owner codes without collapsing conflict truth', () => {
    expect(nativeSalesNotificationFailure(new NativeSalesNotificationError('CUSTOMER_NOTIFICATION_PROVIDER_UNCONFIRMED', 409))).toMatchObject({ status: 409, code: 'CUSTOMER_NOTIFICATION_PROVIDER_UNCONFIRMED' });
    expect(nativeSalesNotificationFailure(new NativeSalesNotificationError('INDEPENDENT_NOTIFICATION_REVIEWER_REQUIRED', 403))).toMatchObject({ status: 403 });
    expect(nativeSalesNotificationFailure(new NativeSalesNotificationError('NATIVE_SALES_NOTIFICATION_RECONCILIATION_UNAVAILABLE', 502))).toMatchObject({ status: 503 });
    expect(nativeSalesNotificationFailure(new NativeSalesNotificationError('NATIVE_SALES_NOTIFICATION_RECONCILIATION_CONTEXT_CHANGED'))).toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
    expect(nativeSalesNotificationFailure(new Error('unrelated'))).toBeUndefined();
  });
});
