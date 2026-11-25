import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
vi.mock('server-only', () => ({}));
import {
  expenseJournalProjection,
  expensePostingAccounts,
  LANE_01_JOURNAL_LINES,
  LANE_01_POSTING_ACCOUNTS,
  nativeLane1Configured,
  type Lane1Actor,
} from '../../../packages/server-finance/src/adapters/native-lane-1/client';
import {
  documentPostingAccounts,
  documentPostingJournalLines,
  documentPostingStockConfiguration,
  DOC_ACCOUNTS,
  DOC_JOURNAL_LINES,
  DOC_STOCK_CONFIGURATION,
} from '../../../packages/server-finance/src/adapters/native-lane-1/doc-preview-client';

const ENV = 'ERPX_NATIVE_LANE_1_READ_CONFIG';
const original = process.env[ENV];
const holdings = [
  { origin: 'https://acme.example.com', host: 'acme.example.com', holdingId: 'h001', holdingSlug: 'acme', companyId: 'acme-co' },
  { origin: 'https://northstar.example.com', host: 'northstar.example.com', holdingId: 'h002', holdingSlug: 'northstar', companyId: 'northstar-co' },
  { origin: 'https://erpx.example.com', host: 'erpx.example.com', holdingId: 'h003', holdingSlug: 'erpx', companyId: 'erpx-co' },
];
const config = JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:44995', timeoutMs: 1000, holdings });
const actors: Lane1Actor[] = holdings.map(h => ({
  holdingId: h.holdingId, holdingSlug: h.holdingSlug, companyId: h.companyId,
}));

const journalRows = [
  { id: 'line-1', code: '1210', name: 'Receivable', debit_minor: 500, credit_minor: 0 },
];
const accountRows = [
  { id: 'acc-1', code: '1210', name: 'Receivable', status: 'active', chart_id: 'chart-1', currency: null },
];

const ok = (kind: string, rows: unknown[]) =>
  new Response(JSON.stringify({ data: { kind, rows } }), {
    status: 200, headers: { 'content-type': 'application/json' },
  });
const sha = (value: string) => createHash('sha256').update(value).digest('hex');

beforeEach(() => { process.env[ENV] = config; });
afterEach(() => {
  if (original === undefined) delete process.env[ENV];
  else process.env[ENV] = original;
  vi.unstubAllGlobals();
});

describe('native Finance lane 1 expense posting projection', () => {
  it('selects each registered Holding by WorkContext and transports the full rows', async () => {
    const calls: { url: URL; init: RequestInit }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (url: URL, init: RequestInit) => {
      calls.push({ url, init });
      const body = JSON.parse(String(init.body)) as { kind: string; journalId?: string };
      return ok(body.kind, body.kind === LANE_01_JOURNAL_LINES ? journalRows : accountRows);
    }));
    const db = {} as never;

    for (let i = 0; i < holdings.length; i += 1) {
      expect(await expenseJournalProjection(db, actors[i]!, 'journal-1')).toEqual(journalRows);
      expect(await expensePostingAccounts(db, actors[i]!)).toEqual(accountRows);
    }

    expect(calls).toHaveLength(6);
    for (let i = 0; i < holdings.length; i += 1) {
      const h = holdings[i]!;
      for (const call of calls.slice(i * 2, i * 2 + 2)) {
        expect(call.url.toString()).toBe(
          `http://127.0.0.1:44995/api/v1/holdings/${h.holdingSlug}/finance/expense-posting-projection`,
        );
        const headers = call.init.headers as Record<string, string>;
        expect(headers.host).toBe(h.host);
        expect(headers.origin).toBe(h.origin);
        expect(headers['x-erpx-context-company']).toBe(h.companyId);
      }
      expect(calls[i * 2]!.init.body).toBe(JSON.stringify({ kind: 'journal-lines', journalId: 'journal-1' }));
      expect(calls[i * 2 + 1]!.init.body).toBe(JSON.stringify({ kind: 'posting-accounts' }));
    }
    expect(nativeLane1Configured()).toBe(true);
  });

  it('takes the Company from the WorkContext when the target omits it', async () => {
    process.env[ENV] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:44995',
      timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.example.com', host: 'acme.example.com', holdingId: 'h001', holdingSlug: 'acme' }],
    });
    vi.stubGlobal('fetch', vi.fn(async () => ok(LANE_01_POSTING_ACCOUNTS, accountRows)));
    await expensePostingAccounts({} as never, { holdingId: 'h001', holdingSlug: 'acme', companyId: 'override-co' });
    const [, init] = vi.mocked(fetch).mock.calls[0] as unknown as [URL, RequestInit];
    expect((init.headers as Record<string, string>)['x-erpx-context-company']).toBe('override-co');
  });

  it('never defaults to the first Holding and fails closed on unknown targets', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    const unknown: Lane1Actor[] = [
      { holdingId: 'h999', holdingSlug: 'acme', companyId: 'acme-co' },
      { holdingId: 'h001', holdingSlug: 'northstar', companyId: 'northstar-co' },
      { holdingId: 'h002', holdingSlug: 'acme', companyId: 'acme-co' },
    ];
    for (const actor of unknown) {
      await expect(expensePostingAccounts({} as never, actor))
        .rejects.toMatchObject({ status: 503, code: 'LANE_01_TARGET_UNAVAILABLE' });
    }
    await expect(expensePostingAccounts({} as never, { holdingId: '', holdingSlug: 'acme' } as Lane1Actor))
      .rejects.toMatchObject({ status: 503, code: 'LANE_01_ACTOR_INVALID' });

    // Neither the WorkContext nor the registered target supplies a Company.
    process.env[ENV] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:44995',
      timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.example.com', host: 'acme.example.com', holdingId: 'h001', holdingSlug: 'acme' }],
    });
    await expect(expensePostingAccounts({} as never, { holdingId: 'h001', holdingSlug: 'acme' }))
      .rejects.toMatchObject({ status: 503, code: 'LANE_01_COMPANY_SCOPE_UNAVAILABLE' });
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('rejects invalid Holding configurations', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    const bad = [
      JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:44995', timeoutMs: 1000, holdings: [] }),
      JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:44995', timeoutMs: 1000, holdings: Array.from({ length: 65 }, (_, i) => ({ origin: `https://h${i}.example.com`, host: `h${i}.example.com`, holdingId: `id${i}`, holdingSlug: `h${i}` })) }),
      JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:44995', timeoutMs: 1000, holdings: [holdings[0], { ...holdings[0]! }] }),
      JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:44995', timeoutMs: 1000, holdings: [{ ...holdings[0]!, host: 'evil.example.com' }] }),
    ];
    for (const raw of bad) {
      process.env[ENV] = raw;
      await expect(expensePostingAccounts({} as never, actors[0]!))
        .rejects.toMatchObject({ status: 503, code: 'LANE_01_READ_CONFIGURATION_INVALID' });
    }
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('pins the SQL and result oracles shared with the Rust lane', () => {
    const src = readFileSync(
      'packages/server-finance/src/adapters/native_lane_1/postgres.rs',
      'utf8',
    );
    const journalSql = src.match(/pub const SQL_JOURNAL_LINES: &str\s*=\s*"([^"]+)"/)?.[1];
    const accountsSql = src.match(/pub const SQL_POSTING_ACCOUNTS: &str\s*=\s*"([^"]+)"/)?.[1];
    expect(journalSql).toBeDefined();
    expect(accountsSql).toBeDefined();
    expect(sha(journalSql!)).toBe('5d0f1e3346f327fd693a72119094b844298e6e57d35552230be1a5c4534714db');
    expect(sha(accountsSql!)).toBe('f8c0fe37f090f6fd6cbeb598c2dc56c56046676d715cb61e675ff4689ba4831f');
    const render = (sql: string) => { let n = 0; return sql.replace(/\?/g, () => `$${++n}`); };
    expect(sha(render(journalSql!))).toBe('e91f823204e9be8bf579a500641156e7482a66fe3c9de11004edf17b1bac7422');
    expect(sha(render(accountsSql!))).toBe('d8c37f12deb8370201967242f2b7817bc9abd9ca008986ef39e03911aacffac5');

    const oracle = {
      kind: 'journal-lines',
      rows: [{ id: 'line-1', code: '1210', name: 'Receivable', debit_minor: 500, credit_minor: 0 }],
    };
    expect(sha(JSON.stringify(oracle))).toBe(
      '0eeb2dec548bcf68643f4b2e6f1040c51420eacb93d4ced04cb48a8b11f51eea',
    );
  });

  it('emits no SQL from the TypeScript adapter or client', () => {
    for (const path of [
      'packages/server-finance/src/adapters/expense-posting-projection.ts',
      'packages/server-finance/src/adapters/native-lane-1/client.ts',
      'packages/server-finance/src/adapters/document-posting-preview.ts',
      'packages/server-finance/src/adapters/native-lane-1/doc-preview-client.ts',
    ]) {
      const src = readFileSync(path, 'utf8');
      expect(src).not.toMatch(/SELECT\s|INSERT\s+INTO|UPDATE\s+\w+\s+SET|DELETE\s+FROM|pg_advisory/i);
    }
  });

  it('rejects a malformed native payload without a TypeScript fallback', async () => {
    const bad = [
      {},
      { data: { kind: 'journal-lines', rows: {} } },
      { data: { kind: 'posting-accounts', rows: journalRows } },
      { data: { kind: 'journal-lines', rows: [{ ...journalRows[0], debit_minor: '500' }] } },
      { data: { kind: 'journal-lines', rows: [{ ...journalRows[0], injected: 'SELECT 1' }] } },
    ];
    for (const payload of bad) {
      vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify(payload), {
        status: 200, headers: { 'content-type': 'application/json' },
      })));
      await expect(expenseJournalProjection({} as never, actors[0]!, 'journal-1'))
        .rejects.toMatchObject({ status: 503, code: 'LANE_01_NATIVE_READ_RESPONSE_INVALID' });
    }
  });

  it('fails closed on missing config, network error and 5xx', async () => {
    delete process.env[ENV];
    await expect(expensePostingAccounts({} as never, actors[0]!))
      .rejects.toMatchObject({ status: 503, code: 'LANE_01_READ_CONFIGURATION_REQUIRED' });

    process.env[ENV] = config;
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('offline'); }));
    await expect(expenseJournalProjection({} as never, actors[0]!, 'journal-1'))
      .rejects.toMatchObject({ status: 503, code: 'LANE_01_NATIVE_READ_UNAVAILABLE' });

    vi.stubGlobal('fetch', vi.fn(async () => new Response('{}', { status: 503 })));
    await expect(expenseJournalProjection({} as never, actors[0]!, 'journal-1'))
      .rejects.toMatchObject({ status: 503 });
  });

  it('propagates an explicit native 4xx rejection and validates journal scope', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({
      error: { code: 'LANE_01_READ_REQUEST_INVALID', message: 'rejected' },
    }), { status: 422, headers: { 'content-type': 'application/json' } })));
    await expect(expenseJournalProjection({} as never, actors[0]!, 'journal-1'))
      .rejects.toMatchObject({ status: 422, code: 'LANE_01_READ_REQUEST_INVALID' });

    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    await expect(expenseJournalProjection({} as never, actors[0]!, ''))
      .rejects.toMatchObject({ status: 503, code: 'LANE_01_JOURNAL_INVALID' });
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

describe('native Finance lane 1 document posting preview', () => {
  const docAccounts = [{ id: 'acc-1', code: '1210', name: 'Receivable', status: 'active' }];
  const docLines = [{ id: 'acc-1', debit: 500, credit: 0 }];
  const docConfig = { inventory_account_id: 'inv', variance_account_id: 'var' };
  const docOk = (kind: string, rows: unknown) =>
    new Response(JSON.stringify({ data: { kind, rows } }), {
      status: 200, headers: { 'content-type': 'application/json' },
    });

  it('routes all three preview reads for every registered Holding', async () => {
    const calls: { url: URL; init: RequestInit }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (url: URL, init: RequestInit) => {
      calls.push({ url, init });
      const kind = String(JSON.parse(String(init.body)).kind);
      return docOk(kind, kind === DOC_ACCOUNTS ? docAccounts : kind === DOC_JOURNAL_LINES ? docLines : docConfig);
    }));
    const db = {} as never;

    for (let i = 0; i < holdings.length; i += 1) {
      const actor = actors[i]!;
      expect(await documentPostingAccounts(db, actor, holdings[i]!.companyId)).toEqual(docAccounts);
      expect(await documentPostingJournalLines(db, actor, 'journal-1')).toEqual(docLines);
      expect(await documentPostingStockConfiguration(db, actor, holdings[i]!.companyId)).toEqual(docConfig);
    }

    expect(calls).toHaveLength(9);
    for (let i = 0; i < holdings.length; i += 1) {
      const h = holdings[i]!;
      for (const call of calls.slice(i * 3, i * 3 + 3)) {
        expect(call.url.toString()).toBe(
          `http://127.0.0.1:44995/api/v1/holdings/${h.holdingSlug}/finance/document-posting-preview`,
        );
        const headers = call.init.headers as Record<string, string>;
        expect(headers.host).toBe(h.host);
        expect(headers.origin).toBe(h.origin);
        expect(headers['x-erpx-context-company']).toBe(h.companyId);
      }
      expect(calls[i * 3]!.init.body).toBe(JSON.stringify({ kind: DOC_ACCOUNTS }));
      expect(calls[i * 3 + 1]!.init.body).toBe(JSON.stringify({ kind: DOC_JOURNAL_LINES, journalId: 'journal-1' }));
      expect(calls[i * 3 + 2]!.init.body).toBe(JSON.stringify({ kind: DOC_STOCK_CONFIGURATION }));
    }
  });

  it('validates preview envelopes strictly and fails closed', async () => {
    const malformed = [
      docOk(DOC_ACCOUNTS, [{ ...docAccounts[0]!, injected: 'SELECT 1' }]),
      docOk(DOC_ACCOUNTS, [{ ...docAccounts[0]!, status: 1 }]),
      docOk(DOC_JOURNAL_LINES, [{ id: 'acc-1', debit: '500', credit: 0 }]),
      docOk(DOC_STOCK_CONFIGURATION, { inventory_account_id: 'inv' }),
      new Response(JSON.stringify({ data: { kind: DOC_ACCOUNTS, rows: {} } }), { status: 200 }),
    ];
    for (const response of malformed) {
      vi.stubGlobal('fetch', vi.fn(async () => response.clone()));
      await expect(documentPostingAccounts({} as never, actors[0]!))
        .rejects.toMatchObject({ status: 503, code: 'LANE_01_DOC_PREVIEW_RESPONSE_INVALID' });
    }
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    await expect(documentPostingAccounts({} as never, { holdingId: 'h999', holdingSlug: 'acme' }))
      .rejects.toMatchObject({ status: 503, code: 'LANE_01_TARGET_UNAVAILABLE' });
    // A different Company yields an owner-scoped empty result, never a query.
    expect(await documentPostingAccounts({} as never, actors[0]!, 'other-co')).toEqual([]);
    expect(await documentPostingStockConfiguration({} as never, actors[0]!, 'other-co')).toBeNull();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('pins the document preview SQL and result oracles', () => {
    const src = readFileSync(
      'packages/server-finance/src/adapters/native_lane_1/doc_preview/postgres.rs',
      'utf8',
    );
    const constant = (name: string) => src.match(new RegExp(`pub const ${name}: &str\\s*=\\s*"([^"]+)"`))?.[1];
    const accounts = constant('SQL_DOC_ACCOUNTS')!;
    const journalOriginal = constant('SQL_DOC_JOURNAL_LINES_ORIGINAL')!;
    const journal = constant('SQL_DOC_JOURNAL_LINES')!;
    const stock = constant('SQL_DOC_STOCK_CONFIGURATION')!;
    expect(accounts).toBeDefined();
    expect(sha(accounts)).toBe('c408c881433c13cdd0eb41702bc621784053af55769857c8b947206ba06a5fcc');
    expect(sha(journalOriginal)).toBe('521029aa4b962b93a4f4bf06b97c10f6edbf565f599f6f36f83ce9c98ceacfa5');
    expect(sha(journal)).toBe('11f89684aee589e754f10306ca8e365d0101ef7879b52d0749e4f8b2b6a31e9e');
    expect(sha(stock)).toBe('62919d5460c74425c09bea6aac557d3ad312537332b2f47333c5617c7b489559');
    expect(sha(JSON.stringify({ kind: DOC_ACCOUNTS, rows: docAccounts }))).toBe(
      '15ead879f6b450d1c847973bfb50bfc56c3b4bfb9d49cde8a384916565413de3',
    );
  });
});
