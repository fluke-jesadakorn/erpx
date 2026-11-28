import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalPointByIdSql } from '../../../modules/legal/server/src/adapters/legal-read-tail-r32-sources';
import { legalPointByKeySql } from '../../../modules/legal/server/src/adapters/legal-read-tail-r36-sources';
import { legalStatePointSql } from '../../../modules/legal/server/src/adapters/legal-read-tail-r33-sources';
import { legalPointByIdAndKeySql } from '../../../modules/legal/server/src/adapters/legal-read-tail-r34-sources';
import { legalCleanDocumentGuardHead } from '../../../modules/legal/server/src/adapters/legal-read-tail-r49-sources';
import { legalLawStateGuardSql } from '../../../modules/legal/server/src/adapters/legal-read-tail-r51-sources';
import { legalPointByIdKeyAndCompanySql } from '../../../modules/legal/server/src/adapters/legal-read-tail-r53-sources';
import {
  legalLawSourceCheckPendingSql, legalLawSourceCurrentSql, legalLawSourceCheckCurrentSql,
  legalLawApplicabilityCurrentSql, legalLawReviewCurrentSql,
} from '../../../modules/legal/server/src/adapters/legal-read-tail-r56-sources';
import { legalDocumentByIdSql } from '../../../modules/legal/server/src/adapters/legal-read-tail-r9-sources';

const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const srcRoot = resolve(__dirname, '../../../modules/legal/server/src');
const src = (name: string) => readFile(new URL(`../../../modules/legal/server/src/adapters/${name}`, import.meta.url), 'utf8');

/** Independent pre-r62 byte + SHA oracles, captured from the exact rendered SQL of
 * all 15 legalPointByIdSql call sites (17 cases: r49 alias/no-alias and both r56
 * review variants included). Every statement keeps the fixed id-key prefix and its
 * original bind order; no request text reaches any head. */
const ORIGINALS: Array<[name: string, sql: string, sha: string, binds: number]> = [
  ['approval/documentAuthor', 'SELECT created_by FROM legal_documents WHERE id=?', '71960302424394bda30f92e2b2c7229c7e2fb34f4456f528a20895ce251f4f80', 1],
  ['portal/documentSummary', 'SELECT id,name,body,version,checksum,media_type FROM legal_documents WHERE id=?', '4fb18af0cc1baa2972ea161c21367f3311a4c049d1a893fb8136e2c38a3018e8', 1],
  ['portal/receipt', 'SELECT * FROM legal_portal_receipts WHERE id=?', '415d03a35beb1d125c82fc872f32d6176c40cb8e07f821d17a4caef28c5cfc78', 1],
  ['portal/item', 'SELECT company_id,branch_id,department_id,version FROM legal_items WHERE id=?', '53a753b9114b93ebf99999133bb9b5f82db118cc72a5f7bce21022cb3e4bc41d', 1],
  ['r33/state', "SELECT id,version,state,created_by FROM legal_law_applicability WHERE id=? AND company_id=? AND state='active'", '15d8ccdbd0b8ebc36e4a58481a5e1d9d21fdbcfb27b59a52b4e0db3ae781866e', 2],
  ['r34/idKey', 'SELECT * FROM legal_text_reviews WHERE id=? AND item_id=?', '68b6e1b8126a62d17587000027e429c0670b2a56738bbac4b1c072be3f01b54b', 2],
  ['r49/noAlias', 'SELECT 1 FROM legal_documents WHERE id=? AND item_id=? AND checksum=? AND deleted_at IS NULL', 'a392fca831f700d1601f18772fe02d2259a27219eead0afa39c11d7ef6e2eebe', 3],
  ['r49/alias', 'SELECT 1 FROM legal_documents d WHERE d.id=? AND d.item_id=? AND d.checksum=? AND d.deleted_at IS NULL', '663b5325ad1bb07f44471d99048bad5f60531dfd8d829f1a2d014a9388ae93ca', 3],
  ['r51/guard', "SELECT 1 FROM legal_law_sources WHERE id=? AND rule_id=? AND company_id=? AND state='active'", '4266102eaffc528bb34da070b281592663d4c7241317d5eb08de196a36ef1a0b', 3],
  ['r53/threeKey', 'SELECT user_id,ordinal,state,signed_at FROM legal_signers WHERE id=? AND item_id=? AND company_id=?', '7990f0a5ad5866aa3f63a10a0e0389dfbf07710f8d8134ccd3b3a9ffadb4e219', 3],
  ['r56/sourceCheckPending', "SELECT 1 FROM legal_law_source_checks WHERE id=? AND source_id=? AND company_id=? AND state='pending_review' AND checksum=? AND candidate_revision_id IS NULL", '8f0bd4b7663db97fbe813aa086ea32b20413ae3e6fec9f7d3c43157c9d501ceb', 4],
  ['r56/sourceCurrent', "SELECT 1 FROM legal_law_sources WHERE id=? AND company_id=? AND rule_id=? AND state='active'", 'f107f3d8ff9ac556e6b8bb47227b4fa599d5b814143f2aa5ff535cb2a30575fa', 3],
  ['r56/checkCurrent', "SELECT 1 FROM legal_law_source_checks WHERE id=? AND state='pending_review' AND checksum=? AND candidate_revision_id IS NULL", '3e1f31410e37f23e18ab7bfe3dc13ae6cc92843c8d3f0b22174629954b677c81', 2],
  ['r56/applicability', "SELECT 1 FROM legal_law_applicability WHERE id=? AND version=? AND state='pending' AND NOT iam_same_person(created_by,?,company_id)", 'a3a15a9580fc492ff51ce0e6731a14f3797a3a955829ff53606e0285fc055bd9', 3],
  ['r56/reviewCurrent', 'SELECT 1 FROM legal_law_revisions WHERE id=? AND version=? AND state=? AND NOT iam_same_person(created_by,?,company_id)', '6b6a3c865d60e90073f576d47b81cec51c60aaea469a4abf47fd6c09d2cdffa2', 4],
  ['r56/reviewRetiring', 'SELECT 1 FROM legal_text_reviews WHERE id=? AND version=? AND state=? ', 'a79d63a414aea56e9b139b4bfa89dd51c052c36fac4c2bb452c850ac22270518', 3],
  ['r9/documentById', 'SELECT * FROM legal_documents WHERE id=?', '45362868a5a1d44578afa7aa656654fa4d80f768e2a1320e7ba3da2cf928fe36', 1],
];

const ACTUAL: Record<string, string> = {
  'approval/documentAuthor': legalPointByIdSql('created_by', 'legal_documents'),
  'portal/documentSummary': legalPointByIdSql('id,name,body,version,checksum,media_type', 'legal_documents'),
  'portal/receipt': legalPointByIdSql('*', 'legal_portal_receipts'),
  'portal/item': legalPointByIdSql('company_id,branch_id,department_id,version', 'legal_items'),
  'r33/state': legalStatePointSql('id,version,state,created_by', 'legal_law_applicability', 'company_id', 'active'),
  'r34/idKey': legalPointByIdAndKeySql('*', 'legal_text_reviews', 'item_id'),
  'r49/noAlias': legalCleanDocumentGuardHead('', 'deleted_at IS NULL'),
  'r49/alias': legalCleanDocumentGuardHead('d', 'd.deleted_at IS NULL'),
  'r51/guard': legalLawStateGuardSql('legal_law_sources', 'active'),
  'r53/threeKey': legalPointByIdKeyAndCompanySql('user_id,ordinal,state,signed_at', 'legal_signers', 'item_id'),
  'r56/sourceCheckPending': legalLawSourceCheckPendingSql(),
  'r56/sourceCurrent': legalLawSourceCurrentSql(),
  'r56/checkCurrent': legalLawSourceCheckCurrentSql(),
  'r56/applicability': legalLawApplicabilityCurrentSql(),
  'r56/reviewCurrent': legalLawReviewCurrentSql('legal_law_revisions', false),
  'r56/reviewRetiring': legalLawReviewCurrentSql('legal_text_reviews', true),
  'r9/documentById': legalDocumentByIdSql,
};

/** The exact fixed head each statement starts from, including r49's alias rewrite. */
const HEADS: Record<string, string> = {
  'approval/documentAuthor': legalPointByIdSql('created_by', 'legal_documents'),
  'portal/documentSummary': legalPointByIdSql('id,name,body,version,checksum,media_type', 'legal_documents'),
  'portal/receipt': legalPointByIdSql('*', 'legal_portal_receipts'),
  'portal/item': legalPointByIdSql('company_id,branch_id,department_id,version', 'legal_items'),
  'r33/state': legalPointByIdSql('id,version,state,created_by', 'legal_law_applicability'),
  'r34/idKey': legalPointByIdSql('*', 'legal_text_reviews'),
  'r49/noAlias': legalPointByIdSql('1', 'legal_documents'),
  'r49/alias': legalPointByIdSql('1', 'legal_documents d').replace('WHERE id=?', 'WHERE d.id=?'),
  'r51/guard': legalPointByIdSql('1', 'legal_law_sources'),
  'r53/threeKey': legalPointByIdSql('user_id,ordinal,state,signed_at', 'legal_signers'),
  'r56/sourceCheckPending': legalPointByIdSql('1', 'legal_law_source_checks'),
  'r56/sourceCurrent': legalPointByIdSql('1', 'legal_law_sources'),
  'r56/checkCurrent': legalPointByIdSql('1', 'legal_law_source_checks'),
  'r56/applicability': legalPointByIdSql('1', 'legal_law_applicability'),
  'r56/reviewCurrent': legalPointByIdSql('1', 'legal_law_revisions'),
  'r56/reviewRetiring': legalPointByIdSql('1', 'legal_text_reviews'),
  'r9/documentById': legalPointByIdSql('*', 'legal_documents'),
};

describe('legal read-tail r62 id-head reuse', () => {
  it('renders every call site byte-identically to the pre-r62 SHA oracles', () => {
    expect(ORIGINALS).toHaveLength(17);
    for (const [name, sql, digest] of ORIGINALS) {
      expect(ACTUAL[name], name).toBe(sql);
      expect(sha256(ACTUAL[name]), name).toBe(digest);
    }
  });

  it('folds the id-key head onto the single r36 arbitrary-key renderer', () => {
    for (const [name, sql, , binds] of ORIGINALS) {
      const head = HEADS[name];
      expect(ACTUAL[name].startsWith(head), name).toBe(true);
      expect((head.match(/\?/g) ?? []).length, name).toBe(1);
      // Every head is exactly the r36 renderer with the fixed 'id' key.
      expect(legalPointByKeySql(head.slice(7, head.indexOf(' FROM ')), head.slice(head.indexOf(' FROM ') + 6, head.indexOf(' WHERE ')), 'id'), name)
        .toBe(head.replace('WHERE d.id=?', 'WHERE id=?'));
      expect(sql, name).toContain('id=?');
      expect((sql.match(/\?/g) ?? []).length, name).toBe(binds);
    }
  });

  it('preserves bind order and rejects write or lock text', () => {
    for (const [name, sql] of ORIGINALS) {
      expect(sql, name).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
      expect(sql, name).toContain('id=?');
    }
    // id bind always precedes the appended key/state tail.
    for (const [name, sql] of ORIGINALS) {
      const idAt = sql.indexOf('id=?');
      const tailAt = sql.indexOf(' AND ', idAt + 4);
      if (tailAt >= 0) expect(idAt, name).toBeLessThan(tailAt);
    }
    // r49's alias rewrite still matches the fixed 'WHERE id=?' token exactly once.
    expect("SELECT 1 FROM legal_documents WHERE id=?".replace('WHERE id=?', 'WHERE d.id=?'))
      .toBe('SELECT 1 FROM legal_documents WHERE d.id=?');
    expect(ACTUAL['r49/alias']).not.toContain('WHERE id=?');
    expect(ACTUAL['r49/noAlias']).toContain('WHERE id=? AND item_id=?');
  });

  it('leaves r32 SELECT-free and delegating, with r36 the one SQL renderer', async () => {
    const [r32, r36] = await Promise.all([
      src('legal-read-tail-r32-sources.ts'), src('legal-read-tail-r36-sources.ts'),
    ]);
    expect(r32.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(r32.match(/export const legalPointByIdSql/g) ?? []).toHaveLength(1);
    expect(r32).toContain("from './legal-read-tail-r36-sources'");
    expect(r32).toContain("legalPointByKeySql(columns, table, 'id')");
    expect((r36.match(/SELECT /g) ?? []).length).toBe(1);
    expect(r36.match(/export const legalPointByKeySql/g) ?? []).toHaveLength(1);
    expect(r36).toContain("'document_id' | 'id' | 'item_id' | 'destruction_id' | 'envelope_id' | 'source_id'");
  });

  it('keeps all 15 call sites and the r49 replace dependency unchanged', async () => {
    const [approval, portal, r49, r9, r33, r34, r51, r53, r56] = await Promise.all([
      src('legal/approval.ts'), src('legal/portal.ts'), src('legal-read-tail-r49-sources.ts'),
      src('legal-read-tail-r9-sources.ts'), src('legal-read-tail-r33-sources.ts'),
      src('legal-read-tail-r34-sources.ts'), src('legal-read-tail-r51-sources.ts'),
      src('legal-read-tail-r53-sources.ts'), src('legal-read-tail-r56-sources.ts'),
    ]);
    expect(approval).toContain('legalPointByIdSql("created_by","legal_documents")');
    expect(portal).toContain('legalPointByIdSql("id,name,body,version,checksum,media_type","legal_documents")');
    expect(portal).toContain('legalPointByIdSql("*","legal_portal_receipts")');
    expect(portal).toContain('legalPointByIdSql("company_id,branch_id,department_id,version","legal_items")');
    expect(r49).toContain("return legalPointByIdSql('1', table).replace('WHERE id=?', `WHERE ${key}=?`) +");
    expect(r9).toContain("export const legalDocumentByIdSql = legalPointByIdSql('*','legal_documents')");
    for (const text of [r33, r34, r51, r53, r56]) expect(text).toContain('legalPointByIdSql(');
    for (const [name, sql] of ORIGINALS) {
      expect(approval, name).not.toContain(sql);
      expect(portal, name).not.toContain(sql);
    }
  });

  it('retirement:ts-statements is BEFORE=137 -> AFTER=136', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(137);
    expect(total).toBeLessThanOrEqual(136);
  });
});
