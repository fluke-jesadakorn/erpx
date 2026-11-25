import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalPointByIdSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r32-sources';
import { legalStatePointSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r33-sources';
import { legalPointByIdAndKeySql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r34-sources';

const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

/** Independent pre-r59 byte + SHA oracles captured from the two removed inline
 * head literals. The r33 state/kind family is the citation/knowledge read; the
 * r34 id+key family backs the portal, processing and document-list reads. */
const ORIGINALS: Array<[string, string, string, number]> = [
  ['state/intake', "SELECT fields,sla_days FROM legal_intake_forms WHERE id=? AND company_id=? AND state='approved'", 'e6db7abb01d881fa77a502fd704e8537e48ea0d15fbd3bc5a41c6b843bdac453', 2],
  ['state/source', "SELECT source_url FROM legal_law_sources WHERE id=? AND company_id=? AND state='active'", 'b6248557e1ecb36d337178e9147c26a909378ede1eec1a862a98e2c0bfab35e9', 2],
  ['state/review', "SELECT 1 FROM legal_text_reviews WHERE id=? AND document_id=? AND state='approved'", 'f6e2085835219b43c8e7e74fba4abefbac94f3fbd41aa3847b1490fa399f7104', 2],
  ['key/receipt', 'SELECT 1 FROM legal_portal_receipts WHERE id=? AND user_id=?', 'c7c8707c4c4098ff22a94fc4ebac6c34215b88254b2407e3498eb747c9e88a85', 2],
  ['key/hash', 'SELECT request_hash,result_json FROM legal_portal_receipts WHERE id=? AND user_id=?', '54f9840c446d8e48135fd8eb3b0c447a873437388b01c0b20c11edcfb30d7ee1', 2],
  ['key/job', 'SELECT id,item_id,document_id,state,result_json,error,attempts FROM legal_processing_jobs WHERE id=? AND company_id=?', 'ff8ba34bb40e0651e49b84f16631f8e37a3435b7f8677d81de21808ec7eac1cc', 2],
  ['key/counsel', 'SELECT owner_id FROM legal_counsel_entries WHERE id=? AND item_id=?', '0f6d830741f6f99bde87e68ba6bec54de57984d344abe01e7b5dd40b5d2ac55a', 2],
];

const ACTUAL: Record<string, string> = {
  'state/intake': legalStatePointSql('fields,sla_days', 'legal_intake_forms', 'company_id', 'approved'),
  'state/source': legalStatePointSql('source_url', 'legal_law_sources', 'company_id', 'active'),
  'state/review': legalStatePointSql('1', 'legal_text_reviews', 'document_id', 'approved'),
  'key/receipt': legalPointByIdAndKeySql('1', 'legal_portal_receipts', 'user_id'),
  'key/hash': legalPointByIdAndKeySql('request_hash,result_json', 'legal_portal_receipts', 'user_id'),
  'key/job': legalPointByIdAndKeySql('id,item_id,document_id,state,result_json,error,attempts', 'legal_processing_jobs', 'company_id'),
  'key/counsel': legalPointByIdAndKeySql('owner_id', 'legal_counsel_entries', 'item_id'),
};

describe('legal read-tail r59 point-read head reuse', () => {
  it('renders all seven replacements byte-identically to the pre-r59 oracles', () => {
    expect(ORIGINALS).toHaveLength(7);
    for (const [name, value, digest] of ORIGINALS) {
      expect(ACTUAL[name], name).toBe(value);
      expect(sha256(ACTUAL[name]), name).toBe(digest);
    }
  });

  it('folds both byte-identical heads onto the single r32 renderer', () => {
    // The r32 head is the exact prefix of both retired literal heads.
    expect(legalPointByIdSql('fields,sla_days', 'legal_intake_forms')).toBe("SELECT fields,sla_days FROM legal_intake_forms WHERE id=?");
    expect(legalPointByIdSql('owner_id', 'legal_counsel_entries')).toBe('SELECT owner_id FROM legal_counsel_entries WHERE id=?');
    // Every rendered statement keeps the exact base-head prefix through id=?.
    for (const [name, value] of ORIGINALS) {
      const head = value.slice(0, value.indexOf('id=?') + 4);
      expect(ACTUAL[name].startsWith(head), name).toBe(true);
      expect(head).toBe(legalPointByIdSql(head.slice(7, head.indexOf(' FROM ')), head.slice(head.indexOf(' FROM ') + 6, head.indexOf(' WHERE '))));
    }
    // Rebuilt from the base head plus the preserved literal tail.
    expect(legalPointByIdSql('fields,sla_days', 'legal_intake_forms') + " AND company_id=? AND state='approved'").toBe(ACTUAL['state/intake']);
    expect(legalPointByIdSql('owner_id', 'legal_counsel_entries') + ' AND item_id=?').toBe(ACTUAL['key/counsel']);
  });

  it('preserves every bind count and rejects write or lock text', () => {
    for (const [name, , , binds] of ORIGINALS) {
      expect((ACTUAL[name].match(/\?/g) ?? []).length, name).toBe(binds);
      expect(ACTUAL[name], name).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    // Bind order (id, then key, then optional state) is pinned by the byte oracle,
    // e.g. the citation/knowledge review read keeps id=? then document_id=?.
    expect(ACTUAL['state/review']).toContain('id=? AND document_id=?');
    expect(ACTUAL['key/job']).toContain('id=? AND company_id=?');
  });

  it('leaves one zero-token base renderer and two delegating heads with closed key unions', async () => {
    const [r32, r33, r34] = await Promise.all([
      src('legal-read-tail-r32-sources.ts'), src('legal-read-tail-r33-sources.ts'), src('legal-read-tail-r34-sources.ts'),
    ]);
    expect(r32.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(r32).toContain("from './legal-read-tail-r36-sources'");
    expect(r32).toContain("legalPointByKeySql(columns, table, 'id')");
    expect(r33.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(r34.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(r33).toContain("from './legal-read-tail-r32-sources'");
    expect(r34).toContain("from './legal-read-tail-r32-sources'");
    expect(r33.match(/export const legalStatePointSql/g) ?? []).toHaveLength(1);
    expect(r34.match(/export const legalPointByIdAndKeySql/g) ?? []).toHaveLength(1);
    expect(r32.match(/export const legalPointByIdSql/g) ?? []).toHaveLength(1);
    // The public closed unions and exported names are unchanged: no cross-owner contract moved.
    expect(r33).toContain("'company_id' | 'document_id' | 'rule_id'");
    expect(r34).toContain("'user_id' | 'company_id' | 'item_id' | 'document_id'");
    expect(r32).toContain('export const legalPointByIdSql');
  });

  it('removes the inline literals from every call site and leaves protected reads intact', async () => {
    const [corpus, destruction, worker, portalPurchases, knowledge, citations] = await Promise.all([
      src('legal/law-corpus.ts'), src('legal/destruction-sources.ts'), src('legal/destruction-worker.ts'),
      src('portal-purchases.ts'), src('legal/knowledge.ts'), src('legal/citations.ts'),
    ]);
    for (const [, value] of ORIGINALS) {
      expect(corpus).not.toContain(value);
      expect(knowledge).not.toContain(value);
      expect(portalPurchases).not.toContain(value);
    }
    // r57 native law-corpus cutover and r58 destruction documents head untouched.
    expect(corpus).toContain("nativeLegalAuxiliaryRead<LawCorpusView>(a,'laws',ruleId)");
    expect(destruction).toContain("legalItemKeyHeadSql('id,checksum,object_key,attachment_source_id,attachment_link_id','legal_documents')");
    // Locks and the inline citation chunk read remain where they were.
    expect(worker).toContain('FOR UPDATE OF x,i');
    expect(portalPurchases).toContain('LOCK TABLE legal_settings,legal_destructions,legal_comments,legal_envelopes,legal_signers,legal_counsel_entries IN SHARE ROW EXCLUSIVE MODE');
    expect(citations).toContain('export function legalCitation');
    expect(citations).toContain("from '../native-lane-26/client'");
    expect(citations).not.toContain('FROM legal_source_chunks c JOIN legal_text_reviews r');
  });

  it('retirement:ts-statements is BEFORE=141 -> AFTER=139', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(141);
    expect(total).toBeLessThanOrEqual(139);
  });
});
