import { createHash } from 'node:crypto';
import { describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  NATIVE_SALES_NOTES_CONFIG_ENV,
  createCustomerNotesClient,
  nativeCustomerNotesConfigured,
  readCustomerNotes,
  type NativeNoteActor,
} from '../../../modules/sales/server/src/adapters/native-lane-30/client';
import { ApiError } from '@erpx/platform-runtime-server/public/http';

const config = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:44996',
  timeoutMs: 1000,
  holdings: [
    {
      origin: 'https://acme.example.com',
      host: 'acme.example.com',
      holdingSlug: 'acme',
      holdingId: 'holding',
    },
  ],
});

/** Exact registered development holdings; the same object is the env JSON. */
const devConfig = {
  privateBaseUrl: 'http://127.0.0.1:3198',
  timeoutMs: 10000,
  holdings: [
    {
      origin: 'https://acme.jesadakorn.com',
      host: 'acme.jesadakorn.com',
      holdingId: 'h001',
      holdingSlug: 'acme',
      companyId: 'acme-co',
    },
    {
      origin: 'https://northstar.jesadakorn.com',
      host: 'northstar.jesadakorn.com',
      holdingId: 'h002',
      holdingSlug: 'northstar',
      companyId: 'northstar-co',
    },
    {
      origin: 'https://erpx-internal.jesadakorn.com',
      host: 'erpx-internal.jesadakorn.com',
      holdingId: 'h003',
      holdingSlug: 'erpx',
      companyId: 'erpx-co',
    },
  ],
};
const devConfigRaw = JSON.stringify(devConfig);
const actorFor = (holdingId: string, holdingSlug: string, companyId: string): NativeNoteActor => ({
  holdingId,
  holdingSlug,
  companyId,
  branchId: null,
  departmentId: null,
});
const payloadFor = (companyId: string) => ({
  notes: [{ ...note, company_id: companyId }],
  history: [history],
});

const actor: NativeNoteActor = {
  holdingId: 'holding',
  holdingSlug: 'acme',
  companyId: 'company',
  branchId: null,
  departmentId: null,
};

const note = {
  id: 'note-1',
  company_id: 'company',
  customer_id: 'customer-1',
  order_id: 'order-1',
  channel: 'phone',
  occurred_at: 1,
  body: 'Called the customer.',
  status: 'active',
  version: 2,
  created_by: 'user-1',
  created_at: 3,
  updated_at: 4,
};
const history = {
  id: 'history-1',
  note_id: 'note-1',
  version: 1,
  action: 'create',
  body: 'Called the customer.',
  reason: 'initial',
  actor_id: 'user-1',
  created_at: 3,
};
const payload = { notes: [note], history: [history] };

const json = (value: unknown): Response =>
  new Response(JSON.stringify(value), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });

async function expect503(call: () => Promise<unknown>, code?: string) {
  try {
    await call();
    throw new Error('expected a fail-closed ApiError');
  } catch (error) {
    expect(error).toBeInstanceOf(ApiError);
    const api = error as ApiError;
    expect(api.status).toBe(503);
    if (code) expect(api.code).toBe(code);
  }
}

describe('native lane 30 customer-note client', () => {
  it('pins the response-shape oracle hash', () => {
    expect(createHash('sha256').update(JSON.stringify(payload)).digest('hex')).toBe(
      '0c2090f04e163b7ad6ac6fe1690a6f6a725caa49addc6568ad2a80ccd68bf926',
    );
  });

  it('calls the native route with bound host/company/scope headers and pins the shape', async () => {
    let url: URL | undefined;
    let headers: Headers | undefined;
    const client = createCustomerNotesClient(config, async (target, sent) => {
      url = target;
      headers = sent;
      return json(payload);
    });
    const result = await client.read(actor, 'order', 'order-1');
    expect(result).toEqual(payload);
    expect(url?.origin).toBe('http://127.0.0.1:44996');
    expect(url?.pathname).toBe('/api/v1/holdings/acme/sales/customer-notes');
    expect(headers?.get('host')).toBe('acme.example.com');
    expect(headers?.get('x-erpx-context-company')).toBe('company');
    expect(headers?.get('x-erpx-note-source')).toBe('order');
    expect(headers?.get('x-erpx-note-id')).toBe('order-1');
    expect(headers?.has('x-erpx-context-branch')).toBe(false);
  });

  it('sends optional branch/department witnesses for scoped actors', async () => {
    let headers: Headers | undefined;
    const client = createCustomerNotesClient(config, async (_url, sent) => {
      headers = sent;
      return json({ notes: [], history: [] });
    });
    await client.read(
      { ...actor, branchId: 'branch-1', departmentId: 'dept-1' },
      'customer',
      'customer-1',
    );
    expect(headers?.get('x-erpx-context-branch')).toBe('branch-1');
    expect(headers?.get('x-erpx-context-department')).toBe('dept-1');
  });

  it('routes every registered Holding from the actor, never the first Holding', async () => {
    const calls: { host: string; path: string; company: string }[] = [];
    const client = createCustomerNotesClient(devConfigRaw, async (url, headers) => {
      const company = headers.get('x-erpx-context-company')!;
      calls.push({ host: headers.get('host')!, path: url.pathname, company });
      return json(payloadFor(company));
    });
    const northstar = await client.read(actorFor('h002', 'northstar', 'northstar-co'), 'order', 'order-1');
    const erpx = await client.read(actorFor('h003', 'erpx', 'erpx-co'), 'order', 'order-1');
    const acme = await client.read(actorFor('h001', 'acme', 'acme-co'), 'order', 'order-1');
    expect(calls).toEqual([
      { host: 'northstar.jesadakorn.com', path: '/api/v1/holdings/northstar/sales/customer-notes', company: 'northstar-co' },
      { host: 'erpx-internal.jesadakorn.com', path: '/api/v1/holdings/erpx/sales/customer-notes', company: 'erpx-co' },
      { host: 'acme.jesadakorn.com', path: '/api/v1/holdings/acme/sales/customer-notes', company: 'acme-co' },
    ]);
    expect(northstar.notes[0].company_id).toBe('northstar-co');
    expect(erpx.notes[0].company_id).toBe('erpx-co');
    expect(acme.notes[0].company_id).toBe('acme-co');
  });

  it('fails closed for an unknown Holding or a companyId pin mismatch without calling the owner', async () => {
    let calls = 0;
    const client = createCustomerNotesClient(devConfigRaw, async () => {
      calls += 1;
      return json({ notes: [], history: [] });
    });
    await expect503(
      () => client.read(actorFor('h999', 'ghost', 'ghost-co'), 'order', 'order-1'),
      'SALES_NATIVE_CUSTOMER_NOTES_TARGET_INVALID',
    );
    await expect503(
      () => client.read(actorFor('h002', 'northstar', 'acme-co'), 'order', 'order-1'),
      'SALES_NATIVE_CUSTOMER_NOTES_TARGET_INVALID',
    );
    expect(calls).toBe(0);
  });

  it('accepts 1..64 holdings and rejects malformed host/companyId config', () => {
    const raw = JSON.parse(devConfigRaw);
    expect(() => createCustomerNotesClient(JSON.stringify({ ...raw, holdings: raw.holdings.slice(0, 1) }))).not.toThrow();
    const boundary = Array.from({ length: 64 }, () => ({ ...raw.holdings[0] }));
    expect(() => createCustomerNotesClient(JSON.stringify({ ...raw, holdings: boundary }))).not.toThrow();
    const tooMany = Array.from({ length: 65 }, () => ({ ...raw.holdings[0] }));
    expect(() => createCustomerNotesClient(JSON.stringify({ ...raw, holdings: tooMany }))).toThrow(ApiError);
    const noHost = raw.holdings.map(({ host: _host, ...rest }: Record<string, unknown>) => rest);
    expect(() => createCustomerNotesClient(JSON.stringify({ ...raw, holdings: noHost }))).toThrow(ApiError);
    const badHost = raw.holdings.map((item: Record<string, unknown>) => ({ ...item, host: 'evil.example.com' }));
    expect(() => createCustomerNotesClient(JSON.stringify({ ...raw, holdings: badHost }))).toThrow(ApiError);
    const badCompany = raw.holdings.map((item: Record<string, unknown>) => ({ ...item, companyId: '' }));
    expect(() => createCustomerNotesClient(JSON.stringify({ ...raw, holdings: badCompany }))).toThrow(ApiError);
  });

  it('rejects malformed, cross-company and wrong-source responses', async () => {
    const bad = [
      { ...payload, extra: 1 },
      { notes: [note], history: {}, },
      { notes: [{ ...note, company_id: 'other' }], history: [] },
      { notes: [{ ...note, order_id: 'different-order' }], history: [] },
      { notes: [{ ...note, unexpected: true }], history: [] },
      { notes: [], history: [{ ...history, version: 0 }] },
    ];
    for (const value of bad) {
      const client = createCustomerNotesClient(config, async () => json(value));
      await expect503(
        () => client.read(actor, 'order', 'order-1'),
        'SALES_NATIVE_CUSTOMER_NOTES_RESPONSE_INVALID',
      );
    }
  });

  it('fails closed on non-200, redirects, wrong content type, transport errors and bad config', async () => {
    const non200 = createCustomerNotesClient(config, async () =>
      new Response(JSON.stringify({ error: { code: 'X' } }), {
        status: 503,
        headers: { 'content-type': 'application/json' },
      }),
    );
    await expect503(() => non200.read(actor, 'order', 'order-1'));

    const wrongType = createCustomerNotesClient(
      config,
      async () => new Response(JSON.stringify(payload), { status: 200, headers: { 'content-type': 'text/plain' } }),
    );
    await expect503(
      () => wrongType.read(actor, 'order', 'order-1'),
      'SALES_NATIVE_CUSTOMER_NOTES_RESPONSE_INVALID',
    );

    const broken = createCustomerNotesClient(config, async () => {
      throw new Error('offline');
    });
    await expect503(() => broken.read(actor, 'order', 'order-1'));

    expect(() => createCustomerNotesClient('not json')).toThrow(ApiError);
    expect(() => createCustomerNotesClient(
      JSON.stringify({ ...JSON.parse(config), privateBaseUrl: 'http://127.0.0.1:3100' }),
    )).toThrow(ApiError);
  });

  it('fails closed through the environment-backed entry point when unconfigured', async () => {
    delete process.env[NATIVE_SALES_NOTES_CONFIG_ENV];
    expect(nativeCustomerNotesConfigured()).toBe(false);
    await expect503(
      () => readCustomerNotes(actor, 'order', 'order-1'),
      'SALES_NATIVE_CUSTOMER_NOTES_CONFIG_REQUIRED',
    );
    process.env[NATIVE_SALES_NOTES_CONFIG_ENV] = config;
    try {
      expect(nativeCustomerNotesConfigured()).toBe(true);
    } finally {
      delete process.env[NATIVE_SALES_NOTES_CONFIG_ENV];
    }
  });
});
