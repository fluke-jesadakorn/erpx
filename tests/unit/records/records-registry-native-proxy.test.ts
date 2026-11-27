import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const root = resolve(__dirname, '../../..');
const read = (path: string) => readFileSync(resolve(root, path), 'utf8');

const config = {
  privateBaseUrl: 'http://127.0.0.1:45991',
  timeoutMs: 1000,
  holdings: [
    {
      origin: 'https://acme.jesadakorn.com',
      holdingSlug: 'acme',
      holdingId: 'h001',
    },
  ],
};

const payload = {
  data: {
    moduleId: 'operations',
    id: 'rec-1',
    sectionId: 'overview',
    title: 'Title',
    description: 'Description',
    status: 'draft',
    fields: [{ key: 'total', label: 'Total', type: 'number', value: 3 }],
    version: 1,
    createdBy: 'actor',
    updatedAt: 7,
  },
  context: {
    userId: 'actor',
    holdingId: 'h001',
    companyId: 'company',
    branchId: null,
    departmentId: null,
    assignmentId: 'assignment-actor',
    assignmentVersion: 3,
    revision: 9,
    role: 'operations_manager',
    scopeKind: 'holding',
    modules: ['operations'],
    permissions: ['records.read'],
    expiresAt: 100,
  },
};

let seen: Array<{ url: URL; init: RequestInit }>;

beforeEach(() => {
  seen = [];
  process.env.ERPX_NATIVE_RECORDS_CONFIG = JSON.stringify(config);
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: string | URL, init: RequestInit) => {
      seen.push({ url: new URL(input), init });
      return new Response(JSON.stringify(payload), {
        status: 200,
        headers: {
          'content-type': 'application/json',
          'cache-control': 'private, no-store',
          'referrer-policy': 'no-referrer',
          'x-content-type-options': 'nosniff',
          'x-internal': 'drop-me',
        },
      });
    }),
  );
});

afterEach(() => {
  delete process.env.ERPX_NATIVE_RECORDS_CONFIG;
  vi.unstubAllGlobals();
});

const request = (
  path: string,
  headers: Record<string, string> = {},
  body?: string,
  method = 'GET',
) =>
  new Request(`https://acme.jesadakorn.com${path}`, {
    method,
    headers: {
      host: 'acme.jesadakorn.com',
      cookie: 'erpx=session',
      origin: 'https://acme.jesadakorn.com',
      'content-type': 'application/json',
      'idempotency-key': 'records:key',
      ...headers,
    },
    body,
  });

const load = async () => {
  const mod = await import(
    '../../../packages/server-records/src/public/native-record-registry'
  );
  return mod;
};

describe('pure Next-to-axum registry transport', () => {
  it('forwards the exact list query and returns the Rust payload verbatim', async () => {
    const { proxyNativeRecordRegistry } = await load();
    const reply = await proxyNativeRecordRegistry(
      request('/api/v1/holdings/acme/records/operations?section=overview&paged=1&q=needle+review&cursor=abc'),
      'acme',
      { method: 'GET', parts: ['operations'], query: true },
    );
    expect(seen[0]!.url.href).toBe(
      'http://127.0.0.1:45991/api/v1/holdings/acme/records/operations?section=overview&paged=1&q=needle+review&cursor=abc',
    );
    expect(seen[0]!.init.method).toBe('GET');
    expect(new Headers(seen[0]!.init.headers).get('cookie')).toBe('erpx=session');
    expect(new Headers(seen[0]!.init.headers).get('host')).toBe('acme.jesadakorn.com');
    expect(seen[0]!.init.body).toBeUndefined();
    expect(await reply.text()).toBe(JSON.stringify(payload));
    expect(reply.status).toBe(200);
    expect(reply.headers.get('content-type')).toBe('application/json');
    expect(reply.headers.get('cache-control')).toBe('private, no-store');
    expect(reply.headers.get('referrer-policy')).toBe('no-referrer');
    expect(reply.headers.get('x-content-type-options')).toBe('nosniff');
    expect(reply.headers.has('x-internal')).toBe(false);
  });

  it('drops the browser-only mention projection query from the context read', async () => {
    const { proxyNativeRecordRegistry } = await load();
    await proxyNativeRecordRegistry(
      request('/api/v1/holdings/acme/records/operations/rec-1?view=mention'),
      'acme',
      { method: 'GET', parts: ['operations', 'rec-1'] },
    );
    expect(seen[0]!.url.href).toBe(
      'http://127.0.0.1:45991/api/v1/holdings/acme/records/operations/rec-1',
    );
  });

  it('forwards commands with body, context headers and expectedVersion unchanged', async () => {
    const { proxyNativeRecordRegistry } = await load();
    const body = JSON.stringify({ expectedVersion: 1, action: 'submit', note: 'Ready' });
    const reply = await proxyNativeRecordRegistry(
      request(
        '/api/v1/holdings/acme/records/support/rec-1/transition?expectedVersion=1',
        {
          'content-type': 'application/json',
          'x-erpx-context-company': 'company',
          'x-erpx-context-assignment': 'assignment-actor',
          'x-erpx-context-revision': '9',
          'x-erpx-context-assignment-version': '3',
          'x-erpx-workspace-branch': 'branch-1',
        },
        body,
        'POST',
      ),
      'acme',
      { method: 'POST', parts: ['support', 'rec-1', 'transition'], query: true, body: true },
    );
    const init = seen[0]!.init;
    const headers = new Headers(init.headers);
    expect(seen[0]!.url.href).toBe(
      'http://127.0.0.1:45991/api/v1/holdings/acme/records/support/rec-1/transition?expectedVersion=1',
    );
    expect(init.method).toBe('POST');
    expect(new TextDecoder().decode(init.body as ArrayBuffer)).toBe(body);
    expect(headers.get('x-erpx-context-revision')).toBe('9');
    expect(headers.get('x-erpx-workspace-branch')).toBe('branch-1');
    expect(headers.get('idempotency-key')).toBe('records:key');
    expect(await reply.json()).toEqual(payload);
  });

  it('maps every native route shape to one axum route', async () => {
    const { proxyNativeRecordRegistry } = await load();
    const cases: Array<[Parameters<typeof proxyNativeRecordRegistry>[2], string]> = [
      [{ method: 'GET', parts: ['operations'] }, '/api/v1/holdings/acme/records/operations'],
      [{ method: 'POST', parts: ['support'], body: true }, '/api/v1/holdings/acme/records/support'],
      [
        { method: 'PATCH', parts: ['support', 'rec-1'], body: true },
        '/api/v1/holdings/acme/records/support/rec-1',
      ],
      [
        { method: 'DELETE', parts: ['support', 'rec-1'], query: true },
        '/api/v1/holdings/acme/records/support/rec-1',
      ],
      [
        { method: 'POST', parts: ['support', 'rec-1', 'references'], body: true },
        '/api/v1/holdings/acme/records/support/rec-1/references',
      ],
      [
        { method: 'POST', parts: ['support', 'rec-1', 'transition'], body: true },
        '/api/v1/holdings/acme/records/support/rec-1/transition',
      ],
      [
        { method: 'GET', parts: ['operations', 'rec-1'] },
        '/api/v1/holdings/acme/records/operations/rec-1',
      ],
    ];
    for (const [options, path] of cases) {
      const reply = await proxyNativeRecordRegistry(request(path), 'acme', options);
      expect(reply.status).toBe(200);
      expect(seen.at(-1)!.url.pathname).toBe(
        `/api/v1/holdings/acme/records/${options.parts.join('/')}`,
      );
    }
    expect(seen).toHaveLength(cases.length);
  });

  it('fails closed without owner work when the config, host or holding is absent', async () => {
    const { proxyNativeRecordRegistry } = await load();
    delete process.env.ERPX_NATIVE_RECORDS_CONFIG;
    const absent = await proxyNativeRecordRegistry(request('/api/v1/holdings/acme/records/operations?section=overview'), 'acme', { method: 'GET', parts: ['operations'], query: true });
    expect(absent.status).toBe(503);
    expect((await absent.json()).error.code).toBe('NATIVE_RECORD_REGISTRY_UNAVAILABLE');
    process.env.ERPX_NATIVE_RECORDS_CONFIG = JSON.stringify(config);
    const wrongHost = await proxyNativeRecordRegistry(
      new Request('https://acme.jesadakorn.com/api/v1/holdings/acme/records/operations', {
        headers: { host: 'unknown.example.test' },
      }),
      'acme',
      { method: 'GET', parts: ['operations'] },
    );
    expect(wrongHost.status).toBe(404);
    const wrongHolding = await proxyNativeRecordRegistry(
      request('/api/v1/holdings/northstar/records/operations'),
      'northstar',
      { method: 'GET', parts: ['operations'] },
    );
    expect(wrongHolding.status).toBe(404);
    expect(seen).toHaveLength(0);
  });

  it('bounds cookies, queries and bodies before reaching axum', async () => {
    const { proxyNativeRecordRegistry } = await load();
    const bigCookie = await proxyNativeRecordRegistry(
      request('/api/v1/holdings/acme/records/operations', { cookie: 'x'.repeat(8193) }),
      'acme',
      { method: 'GET', parts: ['operations'] },
    );
    expect(bigCookie.status).toBe(503);
    const bigQuery = await proxyNativeRecordRegistry(
      request(`/api/v1/holdings/acme/records/operations?section=${'x'.repeat(4096)}`),
      'acme',
      { method: 'GET', parts: ['operations'], query: true },
    );
    expect(bigQuery.status).toBe(422);
    const bigBody = await proxyNativeRecordRegistry(
      request(
        '/api/v1/holdings/acme/records/support',
        { 'content-type': 'application/json' },
        'x'.repeat(64 * 1024 + 1),
        'POST',
      ),
      'acme',
      { method: 'POST', parts: ['support'], body: true },
    );
    expect(bigBody.status).toBe(413);
    expect(seen).toHaveLength(0);
  });

});

describe('Rust owner execution and payload parity pins', () => {
  it('executes owner SQL in the Rust registry adapter under a serializable read-only unit', () => {
    const sql = read('packages/server-records/src/adapters/record_registry.rs');
    expect(sql).toContain('FROM workspace_records WHERE archived_at IS NULL');
    expect(sql).toContain('SHOW transaction_isolation');
    expect(sql).toContain('SHOW transaction_read_only');
    expect(sql).toContain('"serializable"');
    expect(sql).toContain('"on"');
    expect(sql).not.toContain('include_str!');
    const composition = read('src/composition/src/records.rs');
    expect(composition).toContain('impl<C: Clock + Send + Sync + \'static> RecordsRegistryHttpBackend for RecordsBackend<C>');
    expect(composition).toContain('PgRecordRegistry::in_authorized_work(connection)');
    expect(composition).toContain('record_registry_page(');
    expect(composition).toContain('RecordsRegistryHttpResponse {');
  });

  it('pins the exact Rust wire envelope and link-free field set', () => {
    const http = read('packages/server-records/src/adapters/record_registry_http.rs');
    expect(http).toContain('serde_json::json!({"data": self.data, "context": self.context})');
    for (const route of [
      '/api/v1/holdings/{holding_slug}/records/{module_id}',
      '/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}',
      '/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}/references',
      '/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}/transition',
    ])
      expect(http).toContain(route);
    const context = read('packages/server-records/src/domain/record_context.rs');
    for (const field of ['moduleId', 'id', 'sectionId', 'title', 'description', 'status', 'fields', 'version', 'createdBy', 'updatedAt'])
      expect(context).toContain(`"${field}"`);
    expect(context).not.toMatch(/"(?:href|\w*Href|\w*_href)"/);
    const registry = read('packages/server-records/src/domain/record_registry.rs');
    for (const field of ['items', 'nextCursor', 'previousCursor', 'hasMore'])
      expect(registry).toContain(`"${field}"`);
  });
});
