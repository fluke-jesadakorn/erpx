import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalApprovedCycleSql } from '../../../modules/legal/server/src/adapters/legal-read-tail-r22-sources';
import { legalItemKeyHeadSql } from '../../../modules/legal/server/src/adapters/legal-read-tail-r54-sources';
import { legalPointByKeySql } from '../../../modules/legal/server/src/adapters/legal-read-tail-r36-sources';

const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const srcRoot = resolve(__dirname, '../../../modules/legal/server/src');
const src = (name: string) => readFile(new URL(`../../../modules/legal/server/src/adapters/${name}`, import.meta.url), 'utf8');

const APPROVED_TAIL = " AND decision='approved' ORDER BY cycle DESC LIMIT 1";

/** Independent pre-r60 byte + SHA oracles captured from the retired r22 inline literal,
 * one per surviving call site. The documents projection carries three columns; the
 * executed-copies projection carries only cycle. Both bind exactly the item id. */
const ORIGINALS: Array<[name: string, columns: string, sql: string, sha: string, binds: number]> = [
  ['documents', 'cycle,document_id,document_hash', "SELECT cycle,document_id,document_hash FROM legal_approvals WHERE item_id=? AND decision='approved' ORDER BY cycle DESC LIMIT 1", 'd0fdb4bc3a732b686d18d38daa6c21eade0a2176641aff891954ace3157e7481', 1],
  ['executed-copies', 'cycle', "SELECT cycle FROM legal_approvals WHERE item_id=? AND decision='approved' ORDER BY cycle DESC LIMIT 1", '8a785ee052ae40ce25bd05a5f5decb3f74ea84efd9c04bc818ab45f806da59e9', 1],
];

describe('legal read-tail r60 approved-cycle head reuse', () => {
  it('renders both call-site projections byte-identically to the pre-r60 oracles', () => {
    expect(ORIGINALS).toHaveLength(2);
    for (const [name, columns, sql, digest] of ORIGINALS) {
      expect(legalApprovedCycleSql(columns), name).toBe(sql);
      expect(sha256(legalApprovedCycleSql(columns)), name).toBe(digest);
    }
  });

  it('folds both projections onto the single r54 item-key head with no inlined SELECT', () => {
    for (const [name, columns, sql] of ORIGINALS) {
      // Independent base oracle: the r36 arbitrary-key head with the fixed item_id key.
      expect(legalPointByKeySql(columns, 'legal_approvals', 'item_id'), name).toBe(`SELECT ${columns} FROM legal_approvals WHERE item_id=?`);
      expect(legalItemKeyHeadSql(columns, 'legal_approvals'), name).toBe(legalPointByKeySql(columns, 'legal_approvals', 'item_id'));
      expect(legalItemKeyHeadSql(columns, 'legal_approvals') + APPROVED_TAIL, name).toBe(sql);
    }
  });

  it('preserves bind counts and rejects write or lock text', () => {
    for (const [name, columns, sql, , binds] of ORIGINALS) {
      expect((sql.match(/\?/g) ?? []).length, name).toBe(binds);
      expect(legalApprovedCycleSql(columns), name).toBe(sql);
      expect(sql, name).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    // The single bind is the item id, and it precedes the fixed decision tail.
    expect(ORIGINALS[0][2].indexOf('id=?')).toBeLessThan(ORIGINALS[0][2].indexOf('decision='));
  });

  it('leaves one zero-token head with closed delegation and unchanged call sites', async () => {
    const [r22, r54, r36, documents, copies] = await Promise.all([
      src('legal-read-tail-r22-sources.ts'), src('legal-read-tail-r54-sources.ts'), src('legal-read-tail-r36-sources.ts'),
      src('legal/documents.ts'), src('legal/executed-copies.ts'),
    ]);
    expect(r22.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(r22.match(/export const legalApprovedCycleSql/g) ?? []).toHaveLength(1);
    expect(r22).toContain("from './legal-read-tail-r54-sources'");
    expect(r22).toContain("legalItemKeyHeadSql(columns, 'legal_approvals')");
    expect(r54).toContain("from './legal-read-tail-r36-sources'");
    expect(r54).not.toContain('SELECT ');
    expect((r36.match(/SELECT /g) ?? []).length).toBe(1);
    // Both callers keep their exact projections, binds and first<T> shapes.
    expect(documents).toContain("approvedVersion:(facts:{id:string;})=>db.prepare(legalApprovedCycleSql('cycle,document_id,document_hash')).bind(facts.id).first<{cycle:number;document_id:string;document_hash:string}>(),");
    // Lane 26 moved the executed-copy approved-cycle read to the native owner.
    expect(copies).toContain('approval:async id=>(await readFor(id)).approval,');
    expect(copies).not.toContain('legalApprovedCycleSql');
  });

  it('leaves law-corpus, destruction, point heads, citations and locks untouched', async () => {
    const [corpus, destruction, r33, r34, citations, worker, purchases] = await Promise.all([
      src('legal/law-corpus.ts'), src('legal/destruction-sources.ts'),
      src('legal-read-tail-r33-sources.ts'), src('legal-read-tail-r34-sources.ts'),
      src('legal/citations.ts'), src('legal/destruction-worker.ts'), src('portal-purchases.ts'),
    ]);
    // r57 native law-corpus cutover and r58 destruction documents head remain.
    expect(corpus).toContain("nativeLegalAuxiliaryRead<LawCorpusView>(a,'laws',ruleId)");
    expect(destruction).toContain("legalItemKeyHeadSql('id,checksum,object_key,attachment_source_id,attachment_link_id','legal_documents')");
    // r59 point heads remain the single owners of the state / id+key families.
    expect(r33).toContain('export const legalStatePointSql');
    expect(r34).toContain('export const legalPointByIdAndKeySql');
    expect(citations).toContain('export function legalCitation');
    expect(citations).toContain("from '../native-lane-26/client'");
    expect(citations).not.toContain('FROM legal_source_chunks c JOIN legal_text_reviews r');
    expect(worker).toContain('FOR UPDATE OF x,i');
    expect(purchases).toContain('LOCK TABLE legal_settings,legal_destructions,legal_comments,legal_envelopes,legal_signers,legal_counsel_entries IN SHARE ROW EXCLUSIVE MODE');
  });

  it('retirement:ts-statements is BEFORE=139 -> AFTER=138', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(139);
    expect(total).toBeLessThanOrEqual(138);
  });
});
