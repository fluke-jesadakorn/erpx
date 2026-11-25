import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

vi.mock('server-only', () => ({}));

const NOTICE = 'Official-source changes and extracted text remain pending until independently reviewed. Existing deadlines require an explicit owner decision.';
const KEYS = ['sources', 'revisions', 'applicability', 'textReviews', 'checks', 'chunks', 'impacts', 'notice'] as const;
const CONFIG = {
  privateBaseUrl: 'http://127.0.0.1:44999',
  timeoutMs: 2000,
  holdings: [{ origin: 'http://acme.example.test:3443', holdingSlug: 'acme', holdingId: 'holding-a' }],
};
const actor = {
  userId: 'user-one', holdingSlug: 'acme', holdingId: 'holding-a', companyId: 'company-a',
  assignmentId: 'assignment-user-one', branchId: 'branch-a', departmentId: 'dept-a',
  role: 'legal_manager', permissions: ['legal.read'], authority: { revision: 4, assignmentVersion: 2 },
} as unknown as WorkContext;

/** Exact authoritative projection of the native `laws` auxiliary read. */
const native = {
  sources: [{ id: 'source-one', rule_id: 'rule-one', title: 'Gazette' }],
  revisions: [{ id: 'revision-one', rule_id: 'rule-one', state: 'approved' }],
  applicability: [{ id: 'applicability-one', rule_id: 'rule-one', applies: true }],
  textReviews: [{ id: 'review-one', item_id: 'rule-one', state: 'pending' }],
  checks: [{ id: 'check-one', rule_id: 'rule-one', state: 'pending_review' }],
  chunks: [{ item_id: 'rule-one', document_id: 'document-one', page: 1, text: 'synthetic' }],
  impacts: [{ check_id: 'check-one', item_id: 'rule-two', title: 'Impacted rule' }],
  notice: NOTICE,
};

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/${name}`, import.meta.url), 'utf8');

let savedConfig: string | undefined;
let savedFetch: typeof globalThis.fetch;
afterEach(() => {
  globalThis.fetch = savedFetch;
  if (savedConfig === undefined) delete process.env.ERPX_NATIVE_LEGAL_RECORDS_CONFIG;
  else process.env.ERPX_NATIVE_LEGAL_RECORDS_CONFIG = savedConfig;
});
function capture(): { url: string; init?: RequestInit }[] {
  const calls: { url: string; init?: RequestInit }[] = [];
  globalThis.fetch = (async (url: RequestInfo | URL, init?: RequestInit) => {
    calls.push({ url: String(url), init });
    return Response.json(native);
  }) as typeof fetch;
  return calls;
}
function begin(): void {
  savedConfig = process.env.ERPX_NATIVE_LEGAL_RECORDS_CONFIG;
  savedFetch = globalThis.fetch;
  process.env.ERPX_NATIVE_LEGAL_RECORDS_CONFIG = JSON.stringify(CONFIG);
}

describe('legal read-tail r57 native laws cutover', () => {
  it('parity: lawCorpusView returns the exact native projection and shape', async () => {
    const { lawCorpusView } = await import('@erpx/server-legal/public/legal/law-corpus');
    begin();
    const calls = capture();
    const out = await lawCorpusView({} as never, actor);
    expect(out).toEqual(native);
    expect(Object.keys(out as object)).toEqual([...KEYS]);
    expect(calls).toHaveLength(1);
    expect(new URL(calls[0]!.url).pathname).toBe('/api/v1/holdings/acme/legal/laws');
  });

  it('bind: company/branch/department/user and the optional ruleId reach the native laws read', async () => {
    const { nativeLegalAuxiliaryRead } = await import('../../../packages/server-legal/src/adapters/native-legal-auxiliary');
    begin();
    const calls = capture();
    await nativeLegalAuxiliaryRead(actor, 'laws', 'rule/one');
    expect(calls).toHaveLength(1);
    expect(new URL(calls[0]!.url).pathname).toBe('/api/v1/holdings/acme/legal/laws/rule%2Fone');
    const h = new Headers(calls[0]!.init?.headers);
    expect(h.get('x-erpx-context-company')).toBe('company-a');
    expect(h.get('x-erpx-workspace-branch')).toBe('branch-a');
    expect(h.get('x-erpx-workspace-department')).toBe('dept-a');
    expect(h.get('x-erpx-context-assignment')).toBe('assignment-user-one');
    expect(h.get('x-erpx-context-revision')).toBe('4');
    expect(h.get('x-erpx-context-assignment-version')).toBe('2');
    const minimal = { ...actor, branchId: undefined, departmentId: undefined } as unknown as WorkContext;
    await nativeLegalAuxiliaryRead(minimal, 'laws');
    const h2 = new Headers(calls[1]!.init?.headers);
    expect(h2.get('x-erpx-workspace-branch')).toBeNull();
    expect(h2.get('x-erpx-workspace-department')).toBeNull();
    expect(new URL(calls[1]!.url).pathname).toBe('/api/v1/holdings/acme/legal/laws');
  });

  it('fails closed before transport and passes native errors through unchanged', async () => {
    const { lawCorpusView } = await import('@erpx/server-legal/public/legal/law-corpus');
    begin();
    capture();
    const denied = { ...actor, permissions: [] } as unknown as WorkContext;
    await expect(lawCorpusView({} as never, denied)).rejects.toMatchObject({ status: 403, code: 'PERMISSION_REQUIRED' });
    process.env.ERPX_NATIVE_LEGAL_RECORDS_CONFIG = '';
    await expect(lawCorpusView({} as never, actor)).rejects.toMatchObject({ status: 503, code: 'NATIVE_LEGAL_UNAVAILABLE' });
    process.env.ERPX_NATIVE_LEGAL_RECORDS_CONFIG = JSON.stringify(CONFIG);
    globalThis.fetch = (async () => Response.json({ error: { code: 'NOT_FOUND', message: 'Denied' } }, { status: 404 })) as unknown as typeof fetch;
    await expect(lawCorpusView({} as never, actor)).rejects.toMatchObject({ status: 404, code: 'NOT_FOUND', message: 'Denied' });
  });

  it('bind: native law_corpus owns visibility, the five binds, the 200/1000 limits and the notice', async () => {
    const [adapter, domain, application] = await Promise.all([
      src('adapters/postgres_auxiliary_reads.rs'),
      src('domain/auxiliary.rs'),
      src('application/auxiliary.rs'),
    ]);
    const corpus = adapter.slice(adapter.indexOf('fn law_corpus'), adapter.indexOf('fn knowledge'));
    expect(corpus).toContain('self.visibility(actor)');
    expect(corpus).toContain('visibility_for(actor, "rule")');
    expect(corpus).toContain('rows_with_rule(&sql, &company, branch, department, &user, rule_id)');
    expect(corpus).toContain('rows_with_rule(&checks, &company, branch, department, &user, rule_id)');
    expect(corpus).toContain('rows_with_rule(&chunks, &company, branch, department, &user, rule_id)');
    expect(corpus).toContain('.bind(&company)');
    expect(corpus).toContain('.bind(branch)');
    expect(corpus).toContain('.bind(department)');
    expect(corpus).toContain('.bind(&user)');
    expect(corpus).toContain('.bind(rule_id)');
    expect(corpus).toContain('AND i.id=$5');
    expect(corpus).toContain('AND rule.id=$5');
    expect((corpus.match(/LIMIT \{AUXILIARY_ROW_LIMIT\}/g) ?? []).length).toBeGreaterThanOrEqual(3);
    expect(corpus).toContain('LIMIT {AUXILIARY_CHUNK_LIMIT}');
    expect(corpus).toContain(NOTICE);
    expect(domain).toContain('pub const AUXILIARY_ROW_LIMIT: i64 = 200;');
    expect(domain).toContain('pub const AUXILIARY_CHUNK_LIMIT: i64 = 1000;');
    expect(application).toContain('[one] if one == "laws" => LegalAuxiliaryRead::LawCorpus');
  });

  it('KPI: the viewScope repository is gone and TS SELECT statements are 145 -> 142', async () => {
    const corpus = await src('adapters/legal/law-corpus.ts');
    expect(corpus).not.toContain('function viewScope');
    expect(corpus.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(corpus).toContain("nativeLegalAuxiliaryRead<LawCorpusView>(a,'laws',ruleId)");
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(145);
    expect(total).toBe(142);
  });
});
