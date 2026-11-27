import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { ApiError } from '@erpx/server-runtime/public/http';

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
  authority: { sessionId: 's1', revision: 7, assignmentVersion: 3 },
  scope: {
    kind: 'branch',
    holdingId: 'h001',
    holdingSlug: 'acme',
    holdingName: 'Acme',
    companyId: 'acme-co',
    branchId: 'b1',
    branchSlug: 'b1-slug',
    branchName: 'Branch One',
  },
} as unknown as WorkContext;

const departmentActor = {
  ...actor,
  branchId: 'b1',
  departmentId: 'd1',
  scope: {
    kind: 'department',
    holdingId: 'h001',
    holdingSlug: 'acme',
    holdingName: 'Acme',
    companyId: 'acme-co',
    branchId: 'b1',
    branchSlug: 'b1-slug',
    branchName: 'Branch One',
    departmentId: 'd1',
    departmentSlug: 'd1-slug',
    departmentName: 'Department One',
  },
} as unknown as WorkContext;

const cookie = 'erpx_session_acme=s1';
const incomingHeaders = () => new Headers({
  cookie,
  'x-erpx-context-user': 'forged',
  'x-erpx-context-modules': 'finance',
  'x-erpx-workspace-branch': 'forged-branch',
  'x-erpx-context-revision': '999',
  'x-erpx-context-assignment-version': '999',
});

const data = [
  { kind: 'record', module_id: 'operations', id: 'rec-1', title: 'Desk', status: 'draft', version: 3 },
  { kind: 'person', target_id: 'u2', label: 'Alice' },
];

let seen: Array<{ url: URL; init: RequestInit }>;

beforeEach(() => {
  seen = [];
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

  it('resolves the session-backed search through one exact axum route and owner ports', () => {
    const http = read('packages/server-records/src/adapters/record_search_http.rs');
    expect(http).toContain('/api/v1/holdings/{holding_slug}/records/native-search');
    expect(http).toContain('router_for_holdings');
    expect(http).toContain('Vec<(SearchTarget, Arc<dyn RecordSearchBackend>)>');
    expect(http).toContain('backend.search(&host, &headers, &query)');
    expect(read('packages/server-records/src/public/record_search_http.rs')).toContain(
      'RecordSearchBackend, router_for_holdings, validate_targets',
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

  it('mounts the session-backed owner router in the private API', () => {
    const mount = read('src/composition/src/private_development_api.rs');
    expect(mount).toContain('record_search::SearchTarget');
    expect(mount).toContain('record_search_http::RecordSearchBackend');
    expect(mount).toContain('RecordsSearchBackend::new');
    expect(mount).toContain('record_search_http::router_for_holdings');
    expect(mount).toContain('PRIVATE_API_RECORDS_SEARCH_INVALID');
    expect(mount).not.toContain('include_str!');
    const backend = read('src/composition/src/records_search.rs');
    expect(backend).toContain('parse_inbound_account_request');
    expect(backend).toContain('CapturedHoldingMutation::capture');
    expect(backend).toContain('x-erpx-workspace-branch');
    expect(backend).toContain('x-erpx-workspace-department');
  });

});

describe('native Records search transport', () => {
  it('forwards the session cookie and expected selection, never caller identity claims', async () => {
    const { nativeRecordSearch } = await vi.importActual<
      typeof import('../../../packages/server-records/src/public/native-record-search')
    >('../../../packages/server-records/src/public/native-record-search');
    const reply = await nativeRecordSearch(actor, '@user/Alice', incomingHeaders(), async (url, headers) => {
      seen.push({ url, init: { method: 'GET', headers } });
      return new Response(JSON.stringify({ data }), { status: 200, headers: { 'content-type': 'application/json' } });
    });
    expect(seen[0]!.url.origin).toBe(config.privateBaseUrl);
    expect(seen[0]!.url.pathname).toBe('/api/v1/holdings/acme/records/native-search');
    expect(seen[0]!.url.searchParams.get('q')).toBe('@user/Alice');
    expect(seen[0]!.init.method).toBe('GET');
    const headers = new Headers(seen[0]!.init.headers);
    expect(headers.get('host')).toBe('acme.jesadakorn.com');
    expect(headers.get('origin')).toBe('https://acme.jesadakorn.com');
    expect(headers.get('cookie')).toBe(cookie);
    expect(headers.get('x-erpx-context-company')).toBe('acme-co');
    expect(headers.get('x-erpx-context-assignment')).toBe('a1');
    expect(headers.get('x-erpx-context-revision')).toBe('7');
    expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
    expect(headers.get('x-erpx-workspace-branch')).toBe('b1-slug');
    expect(headers.get('x-erpx-workspace-department')).toBeNull();
    expect(headers.get('x-erpx-context-user')).toBeNull();
    expect(headers.get('x-erpx-context-modules')).toBeNull();
    expect(reply).toEqual(data);
  });

  it('carries canonical department scope slugs only from the actor', async () => {
    const { nativeRecordSearch } = await vi.importActual<
      typeof import('../../../packages/server-records/src/public/native-record-search')
    >('../../../packages/server-records/src/public/native-record-search');
    await nativeRecordSearch(departmentActor, 'desk', incomingHeaders(), async (_url, headers) => {
      seen.push({ url: new URL(config.privateBaseUrl), init: { method: 'GET', headers } });
      return new Response(JSON.stringify({ data }), { status: 200, headers: { 'content-type': 'application/json' } });
    });
    const headers = new Headers(seen[0]!.init.headers);
    expect(headers.get('x-erpx-workspace-branch')).toBe('b1-slug');
    expect(headers.get('x-erpx-workspace-department')).toBe('d1-slug');
    expect(headers.get('x-erpx-context-user')).toBeNull();
    expect(headers.get('x-erpx-context-modules')).toBeNull();
  });

  it('rejects a missing session or selection before any transport call', async () => {
    const { nativeRecordSearch } = await vi.importActual<
      typeof import('../../../packages/server-records/src/public/native-record-search')
    >('../../../packages/server-records/src/public/native-record-search');
    const send = vi.fn(async () => new Response(JSON.stringify({ data }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    }));
    await expect(nativeRecordSearch(actor, 'desk', new Headers(), send)).rejects.toMatchObject({
      code: 'NATIVE_RECORDS_SEARCH_SELECTION_REQUIRED',
    });
    const noAuthority = { ...actor, authority: undefined } as unknown as WorkContext;
    await expect(nativeRecordSearch(noAuthority, 'desk', incomingHeaders(), send)).rejects.toMatchObject({
      code: 'NATIVE_RECORDS_SEARCH_SELECTION_REQUIRED',
    });
    expect(send).not.toHaveBeenCalled();
  });

  it('fails closed without a native transport or with a rejected request', async () => {
    const { nativeRecordSearch } = await vi.importActual<
      typeof import('../../../packages/server-records/src/public/native-record-search')
    >('../../../packages/server-records/src/public/native-record-search');
    delete process.env.ERPX_NATIVE_RECORDS_CONFIG;
    await expect(nativeRecordSearch(actor, 'desk', incomingHeaders())).rejects.toMatchObject({
      code: 'NATIVE_RECORDS_SEARCH_UNAVAILABLE',
    });
    process.env.ERPX_NATIVE_RECORDS_CONFIG = JSON.stringify(config);
    const rejected = async () => new Response(JSON.stringify({ error: { code: 'FORBIDDEN' } }), { status: 403, headers: { 'content-type': 'application/json' } });
    await expect(nativeRecordSearch(actor, 'desk', incomingHeaders(), rejected)).rejects.toMatchObject({ status: 403, code: 'FORBIDDEN' });
    expect(ApiError).toBeDefined();
  });
});
