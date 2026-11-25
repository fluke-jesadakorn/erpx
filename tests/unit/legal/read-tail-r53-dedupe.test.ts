import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalPointByIdKeyAndCompanySql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r53-sources';
import { legalPointByIdSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r32-sources';
import { legalPointByIdAndKeySql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r34-sources';
import { legalByIdAndItemSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r27-sources';
import { legalByIdAndCompanySql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r28-sources';
import {
  legalDocumentsItemPairSql,
  legalDocumentByIdSql,
  legalFilesAdvisoryLockSql,
  legalCompanyAdvisoryLockSql,
} from '../../../packages/server-legal/src/adapters/legal-read-tail-r9-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

/** Independent pre-r53 byte oracles captured from the removed inline holders. */
const applicabilitySql = 'SELECT id,version,state,created_by FROM legal_law_applicability WHERE id=? AND rule_id=? AND company_id=?';
const reviewLawSql = 'SELECT * FROM legal_law_revisions WHERE id=? AND rule_id=? AND company_id=?';
const reviewDocSql = 'SELECT * FROM legal_text_reviews WHERE id=? AND item_id=? AND company_id=?';
const byIdAndItem = (table: string) => `SELECT * FROM ${table} WHERE id=? AND item_id=?`;
const byIdAndCompany = (table: string) => `SELECT * FROM ${table} WHERE id=? AND company_id=?`;
const documentsItemPairSql = 'SELECT 1 FROM legal_documents WHERE id=? AND item_id=?';

describe('legal read-tail r53 three-key law reads and id+key head consolidation', () => {
  it('renders byte-identical SQL for all three-key law point reads', () => {
    expect(legalPointByIdKeyAndCompanySql('id,version,state,created_by', 'legal_law_applicability', 'rule_id')).toBe(applicabilitySql);
    expect(legalPointByIdKeyAndCompanySql('*', 'legal_law_revisions', 'rule_id')).toBe(reviewLawSql);
    expect(legalPointByIdKeyAndCompanySql('*', 'legal_text_reviews', 'item_id')).toBe(reviewDocSql);
    // Independent base oracle: r32 alone plus the trusted two-key tail.
    expect(legalPointByIdSql('id,version,state,created_by', 'legal_law_applicability') + ' AND rule_id=? AND company_id=?').toBe(applicabilitySql);
    expect(legalPointByIdSql('*', 'legal_text_reviews') + ' AND item_id=? AND company_id=?').toBe(reviewDocSql);
  });

  it('adds three binds per law read and no write or lock text', () => {
    for (const sql of [applicabilitySql, reviewLawSql, reviewDocSql]) {
      expect((sql.match(/\?/g) ?? []).length).toBe(3);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('adds zero SELECT token and one helper in the new source', async () => {
    const r53 = await src('legal-read-tail-r53-sources.ts');
    expect(r53).not.toContain('SELECT ');
    expect(r53.match(/export const legalPointByIdKeyAndCompanySql/g) ?? []).toHaveLength(1);
    expect(r53).toContain("from './legal-read-tail-r32-sources'");
  });

  it('removes both inline heads from law-corpus and keeps corpus reads and locks intact', async () => {
    const [corpus, r9, worker, citations] = await Promise.all([
      src('legal/law-corpus.ts'), src('legal-read-tail-r9-sources.ts'),
      src('legal/destruction-worker.ts'), src('legal/citations.ts'),
    ]);
    expect(corpus).not.toContain(applicabilitySql);
    expect(corpus).not.toContain('SELECT * FROM ${reviewTables[facts.reviewKind].table} WHERE id=?');
    expect(corpus).toContain("legalPointByIdKeyAndCompanySql('id,version,state,created_by','legal_law_applicability','rule_id')).bind(facts.applicabilityId,facts.id,facts.companyId).first<ReviewRow>()");
    expect(corpus).toContain("legalPointByIdKeyAndCompanySql('*',reviewTables[facts.reviewKind].table,reviewTables[facts.reviewKind].column)).bind(facts.id,facts.id2,facts.companyId).first<ReviewRow>()");
    // Corpus SELECT-only reads, assertions, advisory locks and FOR UPDATE OF locks are untouched.
    expect(corpus).toContain("nativeLegalAuxiliaryRead<LawCorpusView>(a,'laws',ruleId)");
    expect(corpus).toContain('legalCorpusSourceCleanHead(');
    expect(corpus).toContain('requireLawApplicabilityCurrent:');
    expect(corpus).toContain('requireLawReviewCurrent:');
    expect(r9).toContain('legalFilesAdvisoryLockSql');
    expect(r9).toContain('legalCompanyAdvisoryLockSql');
    expect(worker).toContain('FOR UPDATE OF x,i');
    expect(citations).toContain('export function legalCitation');
  });

  it('collapses r27/r28/r9 heads onto r34 with byte-identical rendering', () => {
    for (const table of ['legal_authorities', 'legal_documents', 'legal_destructions']) {
      expect(legalByIdAndItemSql(table)).toBe(byIdAndItem(table));
      expect(legalPointByIdAndKeySql('*', table, 'item_id')).toBe(byIdAndItem(table));
    }
    for (const table of ['legal_intake_forms', 'legal_trust_material']) {
      expect(legalByIdAndCompanySql(table)).toBe(byIdAndCompany(table));
      expect(legalPointByIdAndKeySql('*', table, 'company_id')).toBe(byIdAndCompany(table));
    }
    expect(legalDocumentsItemPairSql).toBe(documentsItemPairSql);
    expect(legalPointByIdAndKeySql('1', 'legal_documents', 'item_id')).toBe(documentsItemPairSql);
    expect(legalDocumentByIdSql).toBe('SELECT * FROM legal_documents WHERE id=?');
  });

  it('keeps exported names as r34-delegating constants with zero new SELECT text', async () => {
    const [r27, r28, r9] = await Promise.all([
      src('legal-read-tail-r27-sources.ts'), src('legal-read-tail-r28-sources.ts'),
      src('legal-read-tail-r9-sources.ts'),
    ]);
    expect(r27).not.toContain('SELECT ');
    expect(r28).not.toContain('SELECT ');
    expect(r27.match(/export const legalByIdAndItemSql/g) ?? []).toHaveLength(1);
    expect(r28.match(/export const legalByIdAndCompanySql/g) ?? []).toHaveLength(1);
    expect(r27).toContain("legalPointByIdAndKeySql('*', table, 'item_id')");
    expect(r28).toContain("legalPointByIdAndKeySql('*', table, 'company_id')");
    expect(r9).toContain("export const legalDocumentsItemPairSql = legalPointByIdAndKeySql('1','legal_documents','item_id')");
    expect(r9).toContain("export const legalDocumentByIdSql = legalPointByIdSql('*','legal_documents')");
  });

  it('retirement:ts-statements stays strictly below the recorded BEFORE=178 / AFTER=173', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(178);
    expect(total).toBeLessThan(173);
  });
});
