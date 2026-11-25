import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { afterEach, describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalRecentItemListSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r31-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');
const recent = (columns: string, table: string, limit: number) =>
  `SELECT ${columns} FROM ${table} WHERE item_id=? ORDER BY created_at DESC LIMIT ${limit}`;

const members: [string, string, number][] = [
  ['id,body,visibility,created_by,created_at', 'legal_comments', 100],
  ['id,state,document_id,expires_at,consent', 'legal_envelopes', 20],
];

describe('legal read-tail r31 parameterized family dedupe', () => {
  it('renders byte-identical recent-item list SQL for both legal detail fact tables', () => {
    for (const [columns, table, limit] of members) {
      expect(legalRecentItemListSql(columns, table, limit)).toBe(recent(columns, table, limit));
    }
  });

  it('keeps the fragment read-only (no writes or row locks)', () => {
    const all = members.map(([c, t, l]) => legalRecentItemListSql(c, t, l)).join(' ');
    expect(all).toMatch(/SELECT \w+(,\w+){4} FROM legal_\w+ WHERE item_id=\? ORDER BY created_at DESC LIMIT \d+/);
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one definition and no inline family literals in the caller', async () => {
    const [shared, service] = await Promise.all([
      src('legal-read-tail-r31-sources.ts'),
      src('legal/service.ts'),
    ]);
    expect(shared.match(/export const legalRecentItemListSql/g) ?? []).toHaveLength(1);
    expect(service).toContain('legalRecentItemListSql("id,body,visibility,created_by,created_at","legal_comments",100)');
    expect(service).toContain('legalRecentItemListSql("id,state,document_id,expires_at,consent","legal_envelopes",20)');
    expect(service).not.toMatch(/SELECT id,body,visibility,created_by,created_at FROM legal_comments WHERE item_id=\? ORDER BY created_at DESC LIMIT 100"/);
    expect(service).not.toMatch(/SELECT id,state,document_id,expires_at,consent FROM legal_envelopes WHERE item_id=\? ORDER BY created_at DESC LIMIT 20"/);
  });

  it('leaves corpus/citation, locks and the r9/r15-r30 families untouched', async () => {
    const names = [
      'legal-read-tail-r9-sources.ts', 'legal-read-tail-r15-sources.ts', 'legal-read-tail-r16-sources.ts',
      'legal-read-tail-r18-sources.ts', 'legal-read-tail-r19-sources.ts', 'legal-read-tail-r20-sources.ts',
      'legal-read-tail-r21-sources.ts', 'legal-read-tail-r22-sources.ts', 'legal-read-tail-r23-sources.ts',
      'legal-read-tail-r24-sources.ts', 'legal-read-tail-r25-sources.ts', 'legal-read-tail-r26-sources.ts',
      'legal-read-tail-r27-sources.ts', 'legal-read-tail-r28-sources.ts', 'legal-read-tail-r29-sources.ts',
      'legal-read-tail-r30-sources.ts',
      'legal/law-corpus.ts', 'legal/citations.ts', 'legal/access.ts', 'legal/destruction-worker.ts',
    ];
    const texts = await Promise.all(names.map(src));
    const symbols = [
      'legalDocumentsItemPairSql', 'legalLaterApprovalGuardSql', 'legalItemSelectSql',
      'legalRetentionPolicySelectSql', 'legalRateCountPrefix', 'legalDisposedDocumentPrefix',
      'legalPurchaseClockCteSql', 'legalPurchaseClockExistsSql', 'legalApprovedCycleSql',
      'legalHoldActiveSql', 'legalHoldTargetGuardPrefix', 'legalJsonbAggByItemSql', 'legalVersionExistsSql',
      'legalByIdAndItemSql', 'legalByIdAndCompanySql', 'legalMemberEditExistsSql', 'legalEventsByItemSql',
      'legalOrderedHistorySql',
    ];
    for (const symbol of symbols) {
      expect(texts.slice(0, 16).join('\n')).toContain(`export const ${symbol}`);
    }
    expect(texts[16]).toContain("nativeLegalAuxiliaryRead<LawCorpusView>(a,'laws',ruleId)");
    expect(texts[17]).toContain('export function legalCitation');
    expect(texts[18]).toContain('legalCompanyAdvisoryLockSql');
    expect(texts[19]).toContain('FOR UPDATE OF x,i');
  });

  it('retirement:ts-statements stays strictly below the recorded BEFORE=242', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(242);
    expect(total).toBeLessThanOrEqual(241);
  });
});

const ENV = 'ERPX_NATIVE_LEGAL_RECORDS_CONFIG';
const CONFIG = {
  privateBaseUrl: 'http://127.0.0.1:44999',
  timeoutMs: 2000,
  holdings: [
    {
      origin: 'http://acme.example.test:3443',
      holdingSlug: 'acme',
      holdingId: 'holding-a',
      companyId: 'company-a',
      assignmentId: 'assignment-a',
    },
    {
      origin: 'http://north.example.test:3443',
      holdingSlug: 'north',
      holdingId: 'holding-b',
      companyId: 'company-b',
    },
  ],
};
const native = { id: 'item-one', version: 7 };
const ownerSrc = (name: string) =>
  readFile(new URL(`../../../packages/server-legal/src/${name}`, import.meta.url), 'utf8');

let savedConfig: string | undefined;
let savedFetch: typeof globalThis.fetch;
afterEach(() => {
  globalThis.fetch = savedFetch;
  if (savedConfig === undefined) delete process.env[ENV];
  else process.env[ENV] = savedConfig;
});
function begin(): void {
  savedConfig = process.env[ENV];
  savedFetch = globalThis.fetch;
  process.env[ENV] = JSON.stringify(CONFIG);
}
function capture(response: () => Response = () => Response.json(native)) {
  const calls: { url: string; init?: RequestInit }[] = [];
  globalThis.fetch = (async (url: RequestInfo | URL, init?: RequestInit) => {
    calls.push({ url: String(url), init });
    return response();
  }) as typeof fetch;
  return calls;
}

describe('legal read-tail r31 native inspection-source cutover', () => {
  it('parity: findInspectionSource returns the exact native projection', async () => {
    const { findInspectionSource } = await import('@erpx/server-legal/public/legal/processing');
    begin();
    const calls = capture();
    const out = await findInspectionSource({} as never, 'company-a', 'doc/one');
    expect(out).toEqual(native);
    expect(calls).toHaveLength(1);
    expect(new URL(calls[0]!.url).pathname).toBe(
      '/api/v1/holdings/acme/legal/inspection-source/doc%2Fone',
    );
  });

  it('bind: the Company selects the exact registered Holding and reaches the owner read', async () => {
    const { nativeInspectionSourceRead } = await import(
      '../../../packages/server-legal/src/adapters/native-legal-auxiliary'
    );
    begin();
    const calls = capture();
    await nativeInspectionSourceRead('company-b', 'doc-one');
    expect(calls).toHaveLength(1);
    const h = new Headers(calls[0]!.init?.headers);
    expect(new URL(calls[0]!.url).pathname).toBe(
      '/api/v1/holdings/north/legal/inspection-source/doc-one',
    );
    expect(h.get('x-erpx-context-company')).toBe('company-b');
    expect(h.get('host')).toBe('north.example.test:3443');
  });

  it('fails closed on unknown Company, invalid ids, missing config and native errors', async () => {
    const { findInspectionSource } = await import('@erpx/server-legal/public/legal/processing');
    begin();
    capture();
    await expect(findInspectionSource({} as never, 'company-x', 'doc-one')).rejects.toMatchObject({
      status: 503,
      code: 'NATIVE_LEGAL_AUTHORITY_INVALID',
    });
    await expect(findInspectionSource({} as never, 'company-a', 'bad id')).rejects.toMatchObject({
      status: 422,
      code: 'NATIVE_LEGAL_READ_REQUEST_INVALID',
    });
    process.env[ENV] = '';
    await expect(findInspectionSource({} as never, 'company-a', 'doc-one')).rejects.toMatchObject({
      status: 503,
      code: 'NATIVE_LEGAL_UNAVAILABLE',
    });
    process.env[ENV] = JSON.stringify(CONFIG);
    capture(() =>
      Response.json({ error: { code: 'NOT_FOUND', message: 'Denied' } }, { status: 404 }),
    );
    await expect(findInspectionSource({} as never, 'company-a', 'doc-one')).rejects.toMatchObject({
      status: 404,
      code: 'NOT_FOUND',
      message: 'Denied',
    });
  });

  it('bind: the Rust owner owns the byte-identical SQL, two binds, enum and path', async () => {
    const [adapter, domain, application] = await Promise.all([
      ownerSrc('adapters/postgres_auxiliary_reads.rs'),
      ownerSrc('domain/auxiliary.rs'),
      ownerSrc('application/auxiliary.rs'),
    ]);
    const sql = adapter.slice(
      adapter.indexOf('const INSPECTION_SOURCE_SQL'),
      adapter.indexOf('const CLEAN_DOCUMENT'),
    );
    expect(sql).toContain(
      'SELECT i.id,i.version::int version FROM legal_items i JOIN legal_documents d',
    );
    expect(sql).toContain('ON d.item_id=i.id WHERE d.id=$1 AND i.company_id=$2');
    expect(sql).not.toContain('?');
    expect(adapter).toContain('.bind(document_id)');
    expect(adapter).toContain('.bind(actor.company_id.as_str())');
    expect(adapter).toContain('fn inspection_source');
    expect(domain).toContain('InspectionSource {');
    expect(domain).toContain('"legal.inspection-source.read"');
    expect(application).toContain('ports.inspection_source(actor, &document_id).await');
    expect(application).toContain('one == "inspection-source"');
  });

  it('KPI: the TypeScript SELECT is deleted and statements are BEFORE=242 -> AFTER=103', async () => {
    const [processing, client] = await Promise.all([
      src('legal/processing.ts'),
      src('native-legal-auxiliary.ts'),
    ]);
    expect(processing).toContain(
      'nativeInspectionSourceRead<{id:string;version:number}>(companyId,documentId)',
    );
    expect(processing).not.toContain('SELECT i.id,i.version::int version');
    expect((processing.match(/SELECT /g) ?? []).length).toBe(0);
    expect(client).toContain("'inspection-source'");
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts'))
          total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(242);
    expect(total).toBeLessThanOrEqual(103);
    expect(total).toBe(103);
  });
});
