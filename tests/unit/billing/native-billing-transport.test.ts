import { describe, expect, it } from 'vitest';
import {
  createNativeBillingClient,
  nativeBillingCommandRequest,
  nativeBillingConfiguration,
  nativeBillingFailure,
  NativeBillingError,
  type NativeBillingConfiguration,
  type NativeBillingPageContext,
  type NativeBillingTransport,
} from '@erpx/server-billing/public/native-billing';
import type { BillingView } from '@erpx/feature-billing/lib/billing';

const actor: NativeBillingPageContext = {
  userId: 'admin',
  holdingId: 'h001',
  holdingSlug: 'acme',
  platform: true,
  role: 'platform_admin',
  assignmentId: 'assignment',
  expiresAt: null,
};

/** Captured owner projection: the exact JSON produced by Rust
 * `BillingActor::context` (`#[serde(rename_all = "camelCase")]`). */
const CAPTURED_CONTEXT =
  '{"userId":"admin","holdingId":"h001","platform":true,"role":"platform_admin","assignmentId":"assignment","expiresAt":4102444800000}';
const EXPIRES = 4_102_444_800_000;

const configuration: NativeBillingConfiguration = {
  privateBaseUrl: 'http://127.0.0.1:3110',
  holdings: [
    {
      origin: 'https://acme.example.test',
      holdingSlug: 'acme',
      holdingId: 'h001',
    },
    {
      origin: 'https://northstar.example.test',
      holdingSlug: 'beta',
      holdingId: 'h002',
    },
  ],
  timeoutMs: 1000,
};

function workspace(overrides: Partial<BillingView> = {}): BillingView {
  return {
    sandbox: true,
    now: 123,
    account: {
      holding_id: 'h001',
      version: 1,
      customer_id: 'customer-1',
      payment_mode: 'manual',
      consent_at: null,
      setup_state: null,
      card_json: null,
    },
    enrollment: {
      id: 'enrollment-1',
      plan_id: 'standard',
      started_at: 100,
      ended_at: null,
      pending_plan: null,
      pending_end: false,
      effective_at: null,
    },
    plans: [
      {
        id: 'standard',
        revision: 1,
        name: 'Standard Support',
        support_satang: 50000,
        effective_month: '2026-01',
      },
    ],
    periods: [
      {
        id: 'period-10',
        month: '2026-10',
        start_at: 100,
        end_at: 200,
        plan_id: 'standard',
        plan_revision: 1,
        support_satang: 50000,
        costs_complete: false,
        closed_at: null,
      },
    ],
    costs: [
      {
        id: 'cost-1',
        period_id: 'period-10',
        category: 'hosting',
        amount_satang: 12500,
        service_start: 100,
        service_end: 200,
        note: 'Provider hosting',
        simulated: false,
        evidence_id: 'evidence-1',
        invoice_id: null,
        original_invoice_id: null,
      },
    ],
    invoices: [
      {
        id: 'invoice-1',
        period_id: 'period-10',
        kind: 'main',
        status: 'open',
        total_satang: 62500,
        lines_json: '[]',
        stripe_id: null,
        hosted_url: null,
        pdf_url: null,
        due_at: null,
        payment_state: null,
        created_at: 500,
        original_invoice_id: null,
      },
    ],
    audit: [
      { id: 'audit-1', actor: 'admin', action: 'cost', reason: 'Reviewed cost', created_at: 500 },
    ],
    jobs: [{ id: 'job-1', state: 'pending', last_error: null }],
    legacy: null,
    ...overrides,
  };
}

function headers(overrides: Record<string, string> = {}) {
  return new Headers({
    host: 'acme.example.test',
    cookie: '__Host-erpx-product=session',
    'x-erpx-platform-assignment': 'assignment',
    ...overrides,
  });
}

function response(data: unknown, status = 200) {
  return new Response(JSON.stringify(data), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function context(overrides: Record<string, unknown> = {}) {
  return {
    userId: actor.userId,
    holdingId: actor.holdingId,
    platform: actor.platform,
    role: actor.role,
    assignmentId: actor.assignmentId,
    expiresAt: EXPIRES,
    ...overrides,
  };
}

describe('native Billing transport', () => {
  it('forwards the exact registered Holding and bounded session context', async () => {
    const calls: Parameters<NativeBillingTransport>[] = [];
    const transport: NativeBillingTransport = async (...args) => {
      calls.push(args);
      return response({ data: workspace(), context: context() });
    };
    const client = createNativeBillingClient(configuration, transport);
    const result = await client.billing(headers(), actor, {
      holdingId: 'h001',
      holdingSlug: 'acme',
    });
    expect(result.account?.version).toBe(1);
    expect(calls).toHaveLength(1);
    const [method, url, sent, body] = calls[0]!;
    expect(method).toBe('GET');
    expect(String(url)).toBe('http://127.0.0.1:3110/api/v1/holdings/acme/billing');
    expect(body).toBeUndefined();
    expect(sent.get('host')).toBe('acme.example.test');
    expect(sent.get('cookie')).toBe('__Host-erpx-product=session');
    expect(sent.get('x-erpx-platform-assignment')).toBe('assignment');
  });

  it('rejects an unknown Host, changed authority or foreign account row', async () => {
    const client = createNativeBillingClient(configuration, async (_, __, ___, ____, signal) => {
      void signal;
      return response({ data: workspace(), context: context() });
    });
    await expect(
      client.billing(headers({ host: 'evil.example.test' }), actor, {
        holdingId: 'h001',
        holdingSlug: 'acme',
      }),
    ).rejects.toMatchObject({ code: 'NATIVE_BILLING_AUTHORITY_INVALID' });

    const changed = createNativeBillingClient(configuration, async () =>
      response({ data: workspace(), context: context({ role: 'platform_auditor' }) }),
    );
    await expect(
      changed.billing(headers(), actor, { holdingId: 'h001', holdingSlug: 'acme' }),
    ).rejects.toMatchObject({ code: 'NATIVE_BILLING_CONTEXT_CHANGED' });

    const foreign = createNativeBillingClient(configuration, async () =>
      response({
        data: workspace({
          account: {
            holding_id: 'h002',
            version: 1,
            customer_id: null,
            payment_mode: 'manual',
            consent_at: null,
            setup_state: null,
            card_json: null,
          },
        }),
        context: context(),
      }),
    );
    await expect(
      foreign.billing(headers(), actor, { holdingId: 'h001', holdingSlug: 'acme' }),
    ).rejects.toMatchObject({ code: 'NATIVE_BILLING_RESPONSE_INVALID' });
  });

  it('rejects extra fields, malformed money, orphan references and order changes', async () => {
    for (const data of [
      { ...workspace(), extra: true },
      workspace({
        costs: [
          {
            id: 'cost-1',
            period_id: 'period-10',
            category: 'hosting',
            amount_satang: -1,
            service_start: 100,
            service_end: 200,
            note: 'Provider hosting',
            simulated: false,
            evidence_id: null,
            invoice_id: null,
            original_invoice_id: null,
          },
        ],
      }),
      workspace({
        costs: [
          {
            id: 'cost-1',
            period_id: 'missing',
            category: 'hosting',
            amount_satang: 100,
            service_start: 100,
            service_end: 200,
            note: 'Provider hosting',
            simulated: false,
            evidence_id: null,
            invoice_id: null,
            original_invoice_id: null,
          },
        ],
      }),
      workspace({
        invoices: [
          {
            id: 'invoice-1',
            period_id: 'period-10',
            kind: 'main',
            status: 'open',
            total_satang: 100,
            lines_json: '{}',
            stripe_id: null,
            hosted_url: null,
            pdf_url: null,
            due_at: null,
            payment_state: null,
            created_at: 500,
            original_invoice_id: null,
          },
        ],
      }),
    ]) {
      const client = createNativeBillingClient(configuration, async () =>
        response({ data, context: context() }),
      );
      await expect(
        client.billing(headers(), actor, { holdingId: 'h001', holdingSlug: 'acme' }),
      ).rejects.toMatchObject({ code: 'NATIVE_BILLING_RESPONSE_INVALID' });
    }
  });

  it('maps owner error bodies and non-JSON failures without leaking provider text', async () => {
    const conflict = createNativeBillingClient(configuration, async () =>
      response(
        { error: { code: 'WORK_CONTEXT_CHANGED', message: 'Reload before retrying.' } },
        409,
      ),
    );
    await expect(
      conflict.billing(headers(), actor, { holdingId: 'h001', holdingSlug: 'acme' }),
    ).rejects.toMatchObject({
      code: 'WORK_CONTEXT_CHANGED',
      status: 409,
    });

    const text = createNativeBillingClient(configuration, async () =>
      new Response('provider secret', {
        status: 502,
        headers: { 'content-type': 'text/plain' },
      }),
    );
    await expect(
      text.billing(headers(), actor, { holdingId: 'h001', holdingSlug: 'acme' }),
    ).rejects.toMatchObject({
      code: 'NATIVE_BILLING_UNAVAILABLE',
      status: 502,
    });
  });

  it('validates loopback private configuration and distinct exact Hosts', () => {
    expect(nativeBillingConfiguration(JSON.stringify(configuration))).toEqual(
      configuration,
    );
    for (const raw of [
      { ...configuration, privateBaseUrl: 'http://127.0.0.1:3100' },
      { ...configuration, privateBaseUrl: 'http://localhost:3110' },
      { ...configuration, privateBaseUrl: 'https://127.0.0.1:3110' },
      {
        ...configuration,
        holdings: [
          configuration.holdings[0]!,
          configuration.holdings[0]!,
        ],
      },
      {
        ...configuration,
        holdings: [{ ...configuration.holdings[0]!, holdingSlug: 'ACME' }],
      },
    ]) {
      expect(() =>
        createNativeBillingClient(
          nativeBillingConfiguration(JSON.stringify(raw)),
          async () => response({ data: workspace(), context: context() }),
        ),
      ).toThrowError(NativeBillingError);
    }
    expect(() => nativeBillingConfiguration('{')).toThrowError(NativeBillingError);
  });

  it('maps bounded failures to the retained page policy', () => {
    expect(
      nativeBillingFailure(new NativeBillingError('NATIVE_BILLING_CONTEXT_CHANGED')),
    ).toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
    expect(nativeBillingFailure(new NativeBillingError('X', 401))).toMatchObject({
      status: 401,
      code: 'UNAUTHENTICATED',
    });
    expect(nativeBillingFailure(new NativeBillingError('X', 403))).toMatchObject({
      status: 403,
      code: 'X',
    });
    expect(nativeBillingFailure(new NativeBillingError('X', 409))).toMatchObject({
      status: 409,
      code: 'X',
    });
    expect(nativeBillingFailure(new NativeBillingError('X', 422))).toMatchObject({
      status: 422,
      code: 'X',
    });
    expect(nativeBillingFailure(new NativeBillingError('X', 500))).toMatchObject({
      status: 503,
      code: 'NATIVE_BILLING_UNAVAILABLE',
    });
    expect(nativeBillingFailure(new Error('other'))).toBeUndefined();
  });

  it('posts exact native cost commands with the retained key and origin', async () => {
    const calls: Parameters<NativeBillingTransport>[] = [];
    const client = createNativeBillingClient(configuration, async (...args) => {
      calls.push(args);
      return response({ data: { ok: true }, context: context() });
    });
    const input = {
      holdingId: 'h001',
      holdingSlug: 'acme',
      action: 'cost' as const,
      key: 'billing-command-0001',
      body: {
        version: 4,
        reason: 'Reviewed actual cost',
        periodId: 'period-10',
        category: 'hosting',
        amountSatang: 12500,
        serviceStart: 100,
        serviceEnd: 200,
        note: 'Provider hosting',
        simulated: false,
        evidenceId: null,
      },
    };
    expect(await client.command(headers(), actor, input)).toEqual({ ok: true });
    expect(calls).toHaveLength(1);
    const [method, url, sent, body] = calls[0]!;
    expect(method).toBe('POST');
    expect(String(url)).toBe('http://127.0.0.1:3110/api/v1/holdings/acme/billing/costs/record');
    expect(sent.get('content-type')).toBe('application/json');
    expect(sent.get('idempotency-key')).toBe('billing-command-0001');
    expect(sent.get('origin')).toBe('https://acme.example.test');
    expect(JSON.parse(body!)).toEqual(input.body);
    await expect(
      client.command(headers({ host: 'evil.example.test' }), actor, input),
    ).rejects.toMatchObject({ code: 'NATIVE_BILLING_AUTHORITY_INVALID' });
    await expect(
      client.command(headers({ origin: 'https://evil.example.test' }), actor, input),
    ).rejects.toMatchObject({ code: 'ORIGIN_FORBIDDEN', status: 403 });
    const invalid = createNativeBillingClient(configuration, async () =>
      response({ data: { ok: false }, context: context() }),
    );
    await expect(invalid.command(headers(), actor, input)).rejects.toMatchObject({
      code: 'NATIVE_BILLING_RESPONSE_INVALID',
    });
  });

  it('posts the exact target provider cost and statement routes and bodies', async () => {
    const calls: Parameters<NativeBillingTransport>[] = [];
    const client = createNativeBillingClient(configuration, async (...args) => {
      calls.push(args);
      return response({ data: { ok: true }, context: context() });
    });
    const cost = {
      holdingId: 'h001',
      holdingSlug: 'acme',
      action: 'verify-provider-cost' as const,
      key: 'billing-provider-cost-0001',
      body: {
        version: 2,
        reason: 'Reviewed provider invoice and FX',
        targetMonth: '2026-12',
        sourceKey: 'a'.repeat(64),
        provider: 'openai',
        statementId: 'stmt-1',
        originalCurrency: 'USD',
        originalMinor: 1000,
        fxNumerator: 36,
        fxDenominator: 1,
        providerCostSatang: 36000,
        evidenceId: 'evidence-1',
      },
    };
    const statement = {
      holdingId: 'h001',
      holdingSlug: 'acme',
      action: 'verify-statement' as const,
      key: 'billing-provider-statement-0001',
      body: {
        version: 2,
        reason: 'Reviewed provider statement',
        targetMonth: '2026-12',
        provider: 'openai',
        statementId: 'stmt-1',
        originalCurrency: 'USD',
        providerTotalMinor: 1000,
        evidenceId: 'evidence-1',
      },
    };
    expect(await client.command(headers(), actor, cost)).toEqual({ ok: true });
    expect(await client.command(headers(), actor, statement)).toEqual({ ok: true });
    expect(calls.map(([, url]) => String(url))).toEqual([
      'http://127.0.0.1:3110/api/v1/holdings/acme/billing/target/provider/cost',
      'http://127.0.0.1:3110/api/v1/holdings/acme/billing/target/provider/statement',
    ]);
    expect(JSON.parse(calls[0]![3]!)).toEqual(cost.body);
    expect(JSON.parse(calls[1]![3]!)).toEqual(statement.body);
  });

  it('posts the exact Billing retry route and body', async () => {
    const calls: Parameters<NativeBillingTransport>[] = [];
    const client = createNativeBillingClient(configuration, async (...args) => {
      calls.push(args);
      return response({ data: { ok: true }, context: context() });
    });
    const input = {
      holdingId: 'h001',
      holdingSlug: 'acme',
      action: 'retry' as const,
      key: 'billing-retry-0001',
      body: {
        jobId: 'job-1',
        reason: 'Retry the reviewed Billing job',
      },
    };
    expect(await client.command(headers(), actor, input)).toEqual({ ok: true });
    const [method, url, sent, body] = calls[0]!;
    expect(method).toBe('POST');
    expect(String(url)).toBe(
      'http://127.0.0.1:3110/api/v1/holdings/acme/billing/jobs/retry',
    );
    expect(sent.get('idempotency-key')).toBe('billing-retry-0001');
    expect(sent.get('origin')).toBe('https://acme.example.test');
    expect(JSON.parse(body!)).toEqual(input.body);
  });

  it('posts the exact target evidence routes and bodies', async () => {
    const calls: Parameters<NativeBillingTransport>[] = [];
    const client = createNativeBillingClient(configuration, async (...args) => {
      calls.push(args);
      return response({ data: { ok: true }, context: context() });
    });
    const base = { holdingId: 'h001', holdingSlug: 'acme', key: 'billing-evidence-0001' };
    const commands = [
      {
        ...base,
        action: 'record-operating-cost' as const,
        body: {
          version: 2,
          reason: 'Reviewed actual cost',
          targetMonth: '2026-09',
          category: 'infrastructure',
          amountSatang: 25000,
          evidenceId: 'evidence-1',
        },
      },
      {
        ...base,
        action: 'review-target-capacity' as const,
        body: {
          version: 2,
          reason: 'Reviewed capacity',
          targetMonth: '2026-12',
          projectedOriginalBytes: 10000000000,
          primaryAvailableBytes: 30000000000,
          backupAvailableBytes: 30000000000,
          reservedBytes: 1000000000,
          companyLimitBytes: 25000000000,
          evidenceId: 'evidence-1',
        },
      },
      {
        ...base,
        action: 'approve-target-extra' as const,
        body: {
          version: 2,
          reason: 'Reviewed extra service',
          targetMonth: '2026-12',
          extraKind: 'setup',
          amountSatang: 10000,
          serviceStart: 1796112000000,
          serviceEnd: 1796198400000,
          note: 'Setup',
          evidenceId: 'evidence-1',
        },
      },
    ];
    for (const command of commands)
      expect(await client.command(headers(), actor, command)).toEqual({ ok: true });
    expect(calls.map(([, url]) => String(url))).toEqual([
      'http://127.0.0.1:3110/api/v1/holdings/acme/billing/target/evidence/operating-cost',
      'http://127.0.0.1:3110/api/v1/holdings/acme/billing/target/evidence/capacity',
      'http://127.0.0.1:3110/api/v1/holdings/acme/billing/target/extra',
    ]);
    expect(calls.map(([, , , body]) => JSON.parse(body!))).toEqual(
      commands.map(command => command.body),
    );
  });

  it('posts the exact target rating route and body', async () => {
    const calls: Parameters<NativeBillingTransport>[] = [];
    const client = createNativeBillingClient(configuration, async (...args) => {
      calls.push(args);
      return response({ data: { ok: true }, context: context() });
    });
    const input = {
      holdingId: 'h001',
      holdingSlug: 'acme',
      action: 'rate-target' as const,
      key: 'billing-rate-0001',
      body: {
        version: 2,
        reason: 'Rated from reviewed target evidence',
        targetMonth: '2027-01',
      },
    };
    expect(await client.command(headers(), actor, input)).toEqual({ ok: true });
    const [method, url, sent, body] = calls[0]!;
    expect(method).toBe('POST');
    expect(String(url)).toBe(
      'http://127.0.0.1:3110/api/v1/holdings/acme/billing/target/rate',
    );
    expect(sent.get('idempotency-key')).toBe('billing-rate-0001');
    expect(sent.get('origin')).toBe('https://acme.example.test');
    expect(JSON.parse(body!)).toEqual(input.body);
  });

  it('posts the exact target-rollout approval route and body', async () => {
    const calls: Parameters<NativeBillingTransport>[] = [];
    const client = createNativeBillingClient(configuration, async (...args) => {
      calls.push(args);
      return response({ data: { ok: true }, context: context() });
    });
    const input = {
      holdingId: 'h001',
      holdingSlug: 'acme',
      action: 'approve-target-rollout' as const,
      key: 'billing-rollout-0001',
      body: {
        version: 2,
        reason: 'Approved rollout after two shadow cycles',
        targetMonth: '2026-12',
        minimumMarginBps: 5000,
      },
    };
    expect(await client.command(headers(), actor, input)).toEqual({ ok: true });
    const [method, url, sent, body] = calls[0]!;
    expect(method).toBe('POST');
    expect(String(url)).toBe(
      'http://127.0.0.1:3110/api/v1/holdings/acme/billing/target/rollout/approve',
    );
    expect(sent.get('idempotency-key')).toBe('billing-rollout-0001');
    expect(sent.get('origin')).toBe('https://acme.example.test');
    expect(JSON.parse(body!)).toEqual(input.body);
  });
  it('maps the exact cost-review command fields onto the owner request body', () => {
    const base = { version: 4, reason: 'Reviewed native command' };
    const cases = [
      [
        {
          action: 'cost' as const, ...base, periodId: 'period-1', costId: 'cost-1',
          category: 'adjustment' as const, amountSatang: -500, serviceStart: 100,
          serviceEnd: 200, note: 'Late adjustment', simulated: true,
          evidenceId: 'evidence-1', originalInvoiceId: 'invoice-1',
        },
        'cost',
        {
          ...base, periodId: 'period-1', costId: 'cost-1', category: 'adjustment',
          amountSatang: -500, serviceStart: 100, serviceEnd: 200,
          note: 'Late adjustment', simulated: true, evidenceId: 'evidence-1',
          originalInvoiceId: 'invoice-1',
        },
      ],
      [
        { action: 'delete-cost' as const, ...base, costId: 'cost-1' },
        'delete-cost',
        { ...base, costId: 'cost-1' },
      ],
      [
        { action: 'complete-costs' as const, ...base, periodId: 'period-1' },
        'complete-costs',
        { ...base, periodId: 'period-1', complete: false },
      ],
    ] as const;
    for (const [input, action, body] of cases)
      expect(nativeBillingCommandRequest(input)).toEqual({ action, body });
    // Target-evidence/provider/rollout/rate actions and retry keep the retained
    // platform-only TypeScript authority; the native client must not map them.
    for (const action of [
      'verify-provider-cost',
      'verify-statement',
      'record-operating-cost',
      'review-target-capacity',
      'approve-target-extra',
      'approve-target-rollout',
      'rate-target',
      'retry',
      'approve',
    ] as const)
      expect(
        nativeBillingCommandRequest({ action, version: 1, reason: 'Retained action' }),
      ).toBeUndefined();
  });

  it('posts the exact invoice create and void routes with the retained key', async () => {
    const calls: Parameters<NativeBillingTransport>[] = [];
    const client = createNativeBillingClient(configuration, async (...args) => {
      calls.push(args);
      return response({ data: { ok: true }, context: context() });
    });
    const approve = {
      holdingId: 'h001',
      holdingSlug: 'acme',
      route: '',
      key: 'billing-command-0001',
      body: { action: 'approve', version: 3, reason: 'Reviewed invoice', periodId: 'period-1' },
    };
    const voided = {
      holdingId: 'h001',
      holdingSlug: 'acme',
      route: '/invoice-1/void',
      key: 'billing-command-0002',
      body: { action: 'void', version: 4, reason: 'Reviewed void', invoiceId: 'invoice-1' },
    };
    expect(await client.invoice(headers(), actor, approve)).toEqual({ ok: true });
    expect(await client.invoice(headers(), actor, voided)).toEqual({ ok: true });
    expect(calls.map(([, url]) => String(url))).toEqual([
      'http://127.0.0.1:3110/api/v1/holdings/acme/billing/invoices',
      'http://127.0.0.1:3110/api/v1/holdings/acme/billing/invoices/invoice-1/void',
    ]);
    expect(calls.map(([, , sent]) => sent.get('idempotency-key'))).toEqual([
      'billing-command-0001',
      'billing-command-0002',
    ]);
    expect(calls.map(([, , , body]) => JSON.parse(body!))).toEqual([
      approve.body,
      voided.body,
    ]);
  });

  it('accepts the exact captured owner BillingContext projection', async () => {
    expect(Object.keys(JSON.parse(CAPTURED_CONTEXT)).sort()).toEqual([
      'assignmentId',
      'expiresAt',
      'holdingId',
      'platform',
      'role',
      'userId',
    ]);
    const client = createNativeBillingClient(configuration, async () =>
      response({ data: { ok: true }, context: JSON.parse(CAPTURED_CONTEXT) }),
    );
    expect(
      await client.command(headers(), actor, {
        holdingId: 'h001',
        holdingSlug: 'acme',
        action: 'cost',
        key: 'billing-command-0001',
        body: {},
      }),
    ).toEqual({ ok: true });
  });

  it('rejects the retired 15-key composition context and any key drift', async () => {
    const { expiresAt, ...missingExpiry } = context();
    for (const value of [
      // The 15-key `context(current)` shape the owner used to emit.
      {
        userId: 'admin',
        holdingId: 'h001',
        companyId: 'erpx',
        branchId: null,
        departmentId: null,
        branchSlug: null,
        departmentSlug: null,
        assignmentId: 'assignment',
        assignmentVersion: 1,
        revision: 1,
        role: 'platform_admin',
        assignmentScopeKind: 'holding',
        modules: [],
        permissions: [],
        expiresAt,
      },
      { ...context(), extra: true },
      missingExpiry,
    ]) {
      const client = createNativeBillingClient(configuration, async () =>
        response({ data: { ok: true }, context: value }),
      );
      await expect(
        client.command(headers(), actor, {
          holdingId: 'h001',
          holdingSlug: 'acme',
          action: 'cost',
          key: 'billing-command-0001',
          body: {},
        }),
      ).rejects.toMatchObject({ code: 'NATIVE_BILLING_RESPONSE_INVALID' });
    }
  });

  it('requires an exact actor expiry when known and a live owner expiry otherwise', async () => {
    const call = (
      ownerContext: Record<string, unknown>,
      caller: NativeBillingPageContext = actor,
    ) => {
      const client = createNativeBillingClient(configuration, async () =>
        response({ data: { ok: true }, context: ownerContext }),
      );
      return client.command(headers(), caller, {
        holdingId: 'h001',
        holdingSlug: 'acme',
        action: 'cost',
        key: 'billing-command-0001',
        body: {},
      });
    };
    for (const ownerContext of [
      context({ expiresAt: null }),
      context({ expiresAt: 1 }),
      context({ expiresAt: Date.now() - 60_000 }),
    ])
      await expect(call(ownerContext)).rejects.toMatchObject({
        code: 'NATIVE_BILLING_CONTEXT_CHANGED',
      });
    const known = { ...actor, expiresAt: EXPIRES };
    expect(await call(context(), known)).toEqual({ ok: true });
    await expect(call(context({ expiresAt: EXPIRES + 1 }), known)).rejects.toMatchObject({
      code: 'NATIVE_BILLING_CONTEXT_CHANGED',
    });
    // Identity drift is never silently accepted, even with a live expiry.
    for (const drift of [
      context({ userId: 'someone-else' }),
      context({ assignmentId: 'other-assignment' }),
      context({ platform: false }),
      context({ role: 'platform_auditor' }),
    ])
      await expect(call(drift)).rejects.toMatchObject({
        code: 'NATIVE_BILLING_CONTEXT_CHANGED',
      });
  });

  it('keeps the exact Host to Holding binding for retained actors', async () => {
    const calls: Parameters<NativeBillingTransport>[] = [];
    const client = createNativeBillingClient(configuration, async (...args) => {
      calls.push(args);
      return response({
        data: { ok: true },
        context: context({ holdingId: 'h002', platform: false, role: 'holding_owner' }),
      });
    });
    const owner = {
      ...actor,
      platform: false,
      role: 'holding_owner',
      holdingId: 'h002',
      holdingSlug: 'beta',
    };
    await expect(
      client.command(headers(), owner, {
        holdingId: 'h002',
        holdingSlug: 'beta',
        action: 'cost',
        key: 'billing-command-0001',
        body: {},
      }),
    ).rejects.toMatchObject({ code: 'NATIVE_BILLING_AUTHORITY_INVALID' });
    expect(calls).toHaveLength(0);

    const sameHolding = createNativeBillingClient(configuration, async (...args) => {
      calls.push(args);
      return response({
        data: { ok: true },
        context: context({ platform: false, role: 'holding_owner' }),
      });
    });
    expect(
      await sameHolding.command(
        headers(),
        { ...actor, platform: false, role: 'holding_owner' },
        {
          holdingId: 'h001',
          holdingSlug: 'acme',
          action: 'cost',
          key: 'billing-command-0001',
          body: {},
        },
      ),
    ).toEqual({ ok: true });
    expect(String(calls[0]![1])).toBe(
      'http://127.0.0.1:3110/api/v1/holdings/acme/billing/costs/record',
    );
    expect(calls[0]![2].get('host')).toBe('acme.example.test');
    expect(calls[0]![2].get('origin')).toBe('https://acme.example.test');
  });

  it('resolves the platform target by immutable Holding id from a Holding surface', async () => {
    const calls: Parameters<NativeBillingTransport>[] = [];
    const client = createNativeBillingClient(configuration, async (...args) => {
      calls.push(args);
      return response({
        data: { ok: true },
        context: context({ holdingId: 'h002' }),
      });
    });
    const platformTarget = { ...actor, holdingId: 'h002', holdingSlug: 'beta' };
    expect(
      await client.command(
        headers({ origin: 'https://acme.example.test' }),
        platformTarget,
        {
          holdingId: 'h002',
          holdingSlug: 'beta',
          action: 'cost',
          key: 'billing-command-0001',
          body: {},
        },
      ),
    ).toEqual({ ok: true });
    expect(String(calls[0]![1])).toBe(
      'http://127.0.0.1:3110/api/v1/holdings/beta/billing/costs/record',
    );
    expect(calls[0]![2].get('host')).toBe('northstar.example.test');
    expect(calls[0]![2].get('origin')).toBe('https://northstar.example.test');
    expect(calls[0]![2].get('cookie')).toBe('__Host-erpx-product=session');

    for (const [incoming, target] of [
      // Not from a configured Holding surface: no authority at all.
      [headers({ host: 'console.example.test' }), platformTarget],
      // Unknown target Holding id.
      [headers(), { ...actor, holdingId: 'h009', holdingSlug: 'nope' }],
      // Foreign browser origin.
      [headers({ origin: 'https://evil.example.test' }), platformTarget],
    ] as const) {
      await expect(
        client.command(incoming, target, {
          holdingId: target.holdingId,
          holdingSlug: target.holdingSlug,
          action: 'cost',
          key: 'billing-command-0001',
          body: {},
        }),
      ).rejects.toMatchObject({
        code:
          incoming.get('origin') === 'https://evil.example.test'
            ? 'ORIGIN_FORBIDDEN'
            : 'NATIVE_BILLING_AUTHORITY_INVALID',
      });
    }
  });

});
