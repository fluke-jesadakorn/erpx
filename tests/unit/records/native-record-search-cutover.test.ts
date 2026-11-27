import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));

const routeCalls = vi.hoisted(() => ({
  auth: [] as string[],
  search: [] as unknown[][],
}));

vi.mock('@erpx/server-identity_access/public/authorization', () => ({
  requireWorkContext: vi.fn(async (holdingSlug: string) => {
    routeCalls.auth.push(holdingSlug);
    return {
      userId: 'u1',
      holdingId: 'h001',
      holdingSlug: 'acme',
      companyId: 'acme-co',
      branchId: 'b1',
      departmentId: null,
      assignmentId: 'a1',
      modules: ['operations'],
    };
  }),
}));

vi.mock('@erpx/server-records/public/native-record-search', () => ({
  nativeRecordSearch: vi.fn(async (...args: unknown[]) => {
    routeCalls.search.push(args);
    return [
      { kind: 'record', module_id: 'operations', id: 'rec-1', title: 'Desk', status: 'draft', version: 3 },
    ];
  }),
}));

import { ApiError } from '@erpx/server-runtime/public/http';
import { GET } from '@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/records/search/route';

const root = resolve(__dirname, '../../..');
const read = (path: string) => readFileSync(resolve(root, path), 'utf8');
const code = (path: string) =>
  read(path)
    .split('\n')
    .filter((line) => !line.trimStart().startsWith('//') && !line.includes('assert!'))
    .join('\n');

const config = {
  privateBaseUrl: 'http://127.0.0.1:45993',
  timeoutMs: 1000,
  holdings: [
    {
      origin: 'https://acme.jesadakorn.com',
      holdingSlug: 'acme',
      holdingId: 'h001',
      companyId: 'acme-co',
    },
  ],
};

const actor = {
  holdingSlug: 'acme',
  holdingId: 'h001',
  companyId: 'acme-co',
  userId: 'u1',
  assignmentId: 'a1',
  branchId: 'b1',
  departmentId: null,
  modules: ['operations'],
} as unknown as WorkContext;

const data = [
  { kind: 'record', module_id: 'operations', id: 'rec-1', title: 'Desk', status: 'draft', version: 3 },
  { kind: 'person', target_id: 'u2', label: 'Alice' },
];

let seen: Array<{ url: URL; init: RequestInit }>;

beforeEach(() => {
  seen = [];
  routeCalls.auth.length = 0;
  routeCalls.search.length = 0;
  process.env.ERPX_NATIVE_RECORDS_CONFIG = JSON.stringify(config);
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: string | URL, init: RequestInit) => {
      seen.push({ url: new URL(input), init });
      return new Response(JSON.stringify({ data }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    }),
  );
});

afterEach(() => {
  delete process.env.ERPX_NATIVE_RECORDS_CONFIG;
  vi.unstubAllGlobals();
});

describe('native Records search owner execution and payload parity', () => {
  it('executes real owner SQL in the Rust adapter, parameterised and finite', () => {
    const sql = code('packages/server-records/src/adapters/record_search.rs');
    for (const needle of [
      'SELECT module_id,id,title,status,version FROM workspace_records',
      'archived_at IS NULL',
      'company_id=$1',
      'module_id=ANY(',
      "module_id||'/'||id ILIKE",
      'title ILIKE',
      'ORDER BY updated_at DESC',
      'LIMIT $',
      'fetch_all(&mut *connection)',
    ]) {
      expect(sql).toContain(needle);
    }
    expect(sql).not.toContain('include_str!');
    expect(sql).not.toContain('SHOW ');
    expect(sql).not.toContain('prepare(');
    expect(sql).toContain('branch_id=$2');
    expect(sql).toContain('department_id=$2');
  });

  it('pins the exact legacy wire fields and envelope', () => {
    const domain = code('packages/server-records/src/domain/record_search.rs');
    for (const field of ['kind', 'module_id', 'id', 'title', 'status', 'version'])
      expect(domain).toContain(`"${field}"`);
    expect(domain).toContain('"target_id"');
    expect(domain).toContain('"label"');
    expect(domain).toContain('"person"');
    expect(domain).toContain('serde_json::json!({"data": self.hits})');
    expect(domain).not.toMatch(/"(?:href|\w*Href|\w*_href)"/);
    expect(domain).toContain('MAX_HITS: usize = 12');
  });

  it('exposes one exact axum route and the owner ports through public', () => {
    const http = read('packages/server-records/src/adapters/record_search_http.rs');
    expect(http).toContain('/api/v1/holdings/{holding_slug}/records/native-search');
    expect(http).toContain('router_for_holdings');
    expect(http).toContain('PgRecordSearch::new(pool, field_probe)');
    expect(http).toContain('handler.execute(&actor, &query, now())');
    expect(read('packages/server-records/src/public/record_search.rs')).toContain(
      'RecordSearchHandler, RecordSearchSource, SearchAdmission, SearchFuture',
    );
    expect(read('packages/server-records/src/public/record_search_http.rs')).toContain(
      'router_for_holdings, router_with_sources, validate_targets',
    );
    for (const [file, needle] of [
      ['domain/mod.rs', 'pub(crate) mod record_search;'],
      ['application/mod.rs', 'pub(crate) mod record_search;'],
      ['adapters/mod.rs', 'pub(crate) mod record_search_http;'],
      ['public/mod.rs', 'pub mod record_search_http;'],
    ] as const) {
      expect(read(`packages/server-records/src/${file}`)).toContain(needle);
    }
  });

  it('mounts the owner router with a composition-owned admission', () => {
    const mount = read('src/composition/src/native_lane_mounts.rs');
    expect(mount).toContain('record_search::SearchTarget');
    expect(mount).toContain('record_search_http::router_for_holdings');
    expect(mount).toContain('field_record_probe("sourceReadScopeProbe")');
    expect(mount).toContain('records_search_admission(pools[0].clone(), pools[i + 1].clone())');
    expect(mount).toContain('PRIVATE_API_RECORDS_SEARCH_INVALID');
    expect(mount).toContain('impl SearchAdmission for RecordsSearchAdmission');
    expect(mount).not.toContain('include_str!');
  });

  it('shared GET forwards q to the native transport and wraps only data', async () => {
    const response = await GET(
      new Request('https://acme.jesadakorn.com/api/v1/holdings/acme/records/search?q=%40user%2FAlice'),
      { params: Promise.resolve({ holdingSlug: 'acme' }) } as never,
    );
    expect(routeCalls.auth).toEqual(['acme']);
    expect(routeCalls.search).toHaveLength(1);
    expect(routeCalls.search[0]![0]).toMatchObject({
      userId: 'u1',
      holdingSlug: 'acme',
      holdingId: 'h001',
      companyId: 'acme-co',
      assignmentId: 'a1',
      modules: ['operations'],
    });
    expect(routeCalls.search[0]![1]).toBe('@user/Alice');
    expect(await response.json()).toEqual({
      data: [
        { kind: 'record', module_id: 'operations', id: 'rec-1', title: 'Desk', status: 'draft', version: 3 },
      ],
    });
  });

  it('an empty q skips the native transport without a TS SQL fallback', async () => {
    const response = await GET(
      new Request('https://acme.jesadakorn.com/api/v1/holdings/acme/records/search'),
      { params: Promise.resolve({ holdingSlug: 'acme' }) } as never,
    );
    expect(routeCalls.auth).toEqual([]);
    expect(routeCalls.search).toEqual([]);
    expect(await response.json()).toEqual({ data: [] });
  });
});

describe('native Records search transport', () => {
  it('forwards q and signed scope, and returns the Rust payload verbatim', async () => {
    const { nativeRecordSearch } = await vi.importActual<
      typeof import('../../../packages/server-records/src/public/native-record-search')
    >('../../../packages/server-records/src/public/native-record-search');
    const reply = await nativeRecordSearch(actor, '@user/Alice', async (url, headers) => {
      seen.push({ url, init: { method: 'GET', headers } });
      return new Response(JSON.stringify({ data }), { status: 200, headers: { 'content-type': 'application/json' } });
    });
    expect(seen[0]!.url.origin).toBe(config.privateBaseUrl);
    expect(seen[0]!.url.pathname).toBe('/api/v1/holdings/acme/records/native-search');
    expect(seen[0]!.url.searchParams.get('q')).toBe('@user/Alice');
    expect(seen[0]!.init.method).toBe('GET');
    const headers = new Headers(seen[0]!.init.headers);
    expect(headers.get('x-erpx-context-user')).toBe('u1');
    expect(headers.get('x-erpx-context-assignment')).toBe('a1');
    expect(headers.get('x-erpx-context-company')).toBe('acme-co');
    expect(headers.get('x-erpx-context-modules')).toBe('operations');
    expect(headers.get('x-erpx-workspace-branch')).toBe('b1');
    expect(reply).toEqual(data);
  });

  it('fails closed without a native transport or with a rejected request', async () => {
    const { nativeRecordSearch } = await vi.importActual<
      typeof import('../../../packages/server-records/src/public/native-record-search')
    >('../../../packages/server-records/src/public/native-record-search');
    delete process.env.ERPX_NATIVE_RECORDS_CONFIG;
    await expect(nativeRecordSearch(actor, 'desk')).rejects.toMatchObject({
      code: 'NATIVE_RECORDS_SEARCH_UNAVAILABLE',
    });
    process.env.ERPX_NATIVE_RECORDS_CONFIG = JSON.stringify(config);
    const rejected = async () => new Response(JSON.stringify({ error: { code: 'FORBIDDEN' } }), { status: 403, headers: { 'content-type': 'application/json' } });
    await expect(nativeRecordSearch(actor, 'desk', rejected)).rejects.toMatchObject({ status: 403, code: 'FORBIDDEN' });
    expect(ApiError).toBeDefined();
  });
});
