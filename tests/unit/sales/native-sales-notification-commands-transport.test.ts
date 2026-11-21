import { afterEach, describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  NativeNotificationCommandError,
  createNativeNotificationCommandClient,
  nativeNotificationCommandFailure,
  notificationQueueInput,
  notificationRetryInput,
  notificationSettingsInput,
  type NativeNotificationPageContext,
} from '../../../packages/feature-sales/src/public/native-sales-notification-commands';

const incoming = (overrides: Record<string, string> = {}) =>
  new Headers({ host: 'acme.example.com', cookie: '__Host-erpx-product=session', ...overrides });
const actor: NativeNotificationPageContext = {
  userId: 'owner',
  holdingId: 'h001',
  holdingSlug: 'acme',
  companyId: 'company',
  branchId: null,
  departmentId: null,
  assignmentId: 'assignment-owner',
  role: 'sales_manager',
  assignmentScope: 'holding',
  authority: { revision: 7, assignmentVersion: 3 },
  modules: ['sales'],
  iam: { permissions: ['commerce.read', 'commerce.sales', 'commerce.approve'] },
};
const context = {
  userId: 'owner',
  holdingId: 'h001',
  companyId: 'company',
  branchId: null,
  departmentId: null,
  branchSlug: null,
  departmentSlug: null,
  assignmentId: 'assignment-owner',
  assignmentVersion: 3,
  revision: 7,
  role: 'sales_manager',
  assignmentScopeKind: 'holding',
  modules: ['sales'],
  permissions: ['commerce.read', 'commerce.sales', 'commerce.approve'],
  expiresAt: 9_007_199_254_740_991,
};
const config = {
  privateBaseUrl: 'http://127.0.0.1:44994',
  timeoutMs: 1000,
  holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
};
const ok = (data: unknown) =>
  new Response(JSON.stringify({ data, context }), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });

afterEach(() => vi.unstubAllEnvs());

describe('native Sales notification command transport', () => {
  it('forwards settings queue and retry to the exact native paths', async () => {
    const transport = vi
      .fn()
      .mockResolvedValueOnce(ok({ companyId: 'company', adapter: 'sandbox', version: 1 }))
      .mockResolvedValueOnce(ok({ id: 'customer-notification-1', orderId: 'order-1', status: 'pending', version: 1 }))
      .mockResolvedValueOnce(ok({ id: 'notice-1', orderId: 'order-1', status: 'pending', version: 3 }));
    const client = createNativeNotificationCommandClient(config, transport);
    expect(
      await client.settings(incoming(), actor, { expectedVersion: 0, adapter: 'sandbox' }, 'notification-settings-0001'),
    ).toEqual({ companyId: 'company', adapter: 'sandbox', version: 1 });
    expect(transport.mock.calls[0][1].toString()).toBe(
      'http://127.0.0.1:44994/api/v1/holdings/acme/customer-notifications/settings',
    );
    await client.queue(
      incoming(),
      actor,
      'fulfillment-1',
      { expectedVersion: 1, note: 'Carrier confirmed the delivery window' },
      'notification-queue-0001',
    );
    expect(transport.mock.calls[1][1].toString()).toBe(
      'http://127.0.0.1:44994/api/v1/holdings/acme/fulfillments/fulfillment-1/notify',
    );
    await client.retry(incoming(), actor, 'notice-1', { expectedVersion: 2 }, 'notification-retry-0001');
    expect(transport.mock.calls[2][1].toString()).toBe(
      'http://127.0.0.1:44994/api/v1/holdings/acme/customer-notifications/notice-1/retry',
    );
    expect(transport.mock.calls[0][2].get('idempotency-key')).toBe('notification-settings-0001');
    expect(transport.mock.calls[0][2].get('cookie')).toBe('__Host-erpx-product=session');
  });

  it('strictly validates settings queue and retry bodies', () => {
    expect(notificationSettingsInput({ expectedVersion: 0, adapter: 'disabled' })).toEqual({ expectedVersion: 0, adapter: 'disabled' });
    expect(notificationQueueInput({ expectedVersion: 1, note: 'Call the customer' })).toEqual({ expectedVersion: 1, note: 'Call the customer' });
    expect(notificationRetryInput({ expectedVersion: 1 })).toEqual({ expectedVersion: 1 });
    for (const [validate, invalid] of [
      [notificationSettingsInput, { expectedVersion: -1, adapter: 'sandbox' }],
      [notificationSettingsInput, { expectedVersion: 0, adapter: 'other' }],
      [notificationSettingsInput, { expectedVersion: 0, adapter: 'sandbox', extra: true }],
      [notificationQueueInput, { expectedVersion: 1, note: 'x' }],
      [notificationQueueInput, { expectedVersion: 0, note: 'Call the customer' }],
      [notificationRetryInput, { expectedVersion: 1, extra: true }],
    ] as const) {
      let error: unknown;
      try {
        (validate as (value: unknown) => unknown)(invalid);
      } catch (caught) {
        error = caught;
      }
      expect(error).toBeInstanceOf(NativeNotificationCommandError);
      expect((error as NativeNotificationCommandError).code).toBe('CUSTOMER_NOTIFICATION_REQUEST_INVALID');
    }
  });

  it('fails closed on foreign hosts, context drift, invalid receipts and owner errors', async () => {
    const client = (response: Response) =>
      createNativeNotificationCommandClient(config, vi.fn().mockResolvedValue(response));
    await expect(
      client(ok({ companyId: 'company', adapter: 'sandbox', version: 1 })).settings(
        incoming({ host: 'other.example.com' }),
        actor,
        { expectedVersion: 0, adapter: 'sandbox' },
        'notification-settings-0001',
      ),
    ).rejects.toMatchObject({ code: 'NATIVE_SALES_NOTIFICATION_COMMANDS_AUTHORITY_INVALID' });
    await expect(
      client(
        new Response(JSON.stringify({ data: { companyId: 'company', adapter: 'sandbox', version: 1 }, context: { ...context, revision: 8 } }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        }),
      ).settings(incoming(), actor, { expectedVersion: 0, adapter: 'sandbox' }, 'notification-settings-0001'),
    ).rejects.toMatchObject({ code: 'NATIVE_SALES_NOTIFICATION_COMMANDS_CONTEXT_CHANGED' });
    await expect(
      client(ok({ id: 'notice-1', orderId: 'order-1', status: 'sent', version: 1 })).queue(
        incoming(),
        actor,
        'fulfillment-1',
        { expectedVersion: 1, note: 'Carrier confirmed the delivery window' },
        'notification-queue-0001',
      ),
    ).rejects.toMatchObject({ code: 'NATIVE_SALES_NOTIFICATION_COMMANDS_RESPONSE_INVALID' });
    await expect(
      client(
        new Response(JSON.stringify({ error: { code: 'FEATURE_COMING_SOON', message: 'Synthetic release refusal.' } }), {
          status: 403,
          headers: { 'content-type': 'application/json' },
        }),
      ).settings(incoming(), actor, { expectedVersion: 0, adapter: 'sandbox' }, 'notification-settings-0001'),
    ).rejects.toMatchObject({ code: 'FEATURE_COMING_SOON', status: 403 });
  });

  it('maps owner codes without collapsing conflict truth', () => {
    expect(nativeNotificationCommandFailure(new NativeNotificationCommandError('VERSION_CONFLICT', 409))).toMatchObject({ status: 409 });
    expect(nativeNotificationCommandFailure(new NativeNotificationCommandError('CUSTOMER_ALREADY_NOTIFIED', 409))).toMatchObject({ status: 409 });
    expect(nativeNotificationCommandFailure(new NativeNotificationCommandError('NATIVE_SALES_NOTIFICATION_COMMANDS_UNAVAILABLE', 502))).toMatchObject({ status: 503 });
    expect(nativeNotificationCommandFailure(new NativeNotificationCommandError('NATIVE_SALES_NOTIFICATION_COMMANDS_CONTEXT_CHANGED'))).toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
    expect(nativeNotificationCommandFailure(new Error('unrelated'))).toBeUndefined();
  });
});
