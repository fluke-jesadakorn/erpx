import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  legalLawSourceCheckPendingSql,
  legalLawSourceCurrentSql,
  legalLawSourceCheckCurrentSql,
  legalLawApplicabilityCurrentSql,
  legalLawReviewCurrentSql,
  legalLawSourceCheckUnchangedSql,
  legalCounselConflictClearedSql,
  legalCounselApprovedFeesSql,
} from '../../../packages/server-legal/src/adapters/legal-read-tail-r56-sources';

const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

/** Independent byte + SHA oracles captured from the inline holders at HEAD
 * (BEFORE=153) before they were routed through the closed r56 heads. The
 * law-review oracles include the exact trailing space of the retiring arm. */
const ORIGINALS = [
  ['lawSourceCheckPending', "SELECT 1 FROM legal_law_source_checks WHERE id=? AND source_id=? AND company_id=? AND state='pending_review' AND checksum=? AND candidate_revision_id IS NULL", '8f0bd4b7663db97fbe813aa086ea32b20413ae3e6fec9f7d3c43157c9d501ceb', 4],
  ['lawSourceCurrent', "SELECT 1 FROM legal_law_sources WHERE id=? AND company_id=? AND rule_id=? AND state='active'", 'f107f3d8ff9ac556e6b8bb47227b4fa599d5b814143f2aa5ff535cb2a30575fa', 3],
  ['lawSourceCheckCurrent', "SELECT 1 FROM legal_law_source_checks WHERE id=? AND state='pending_review' AND checksum=? AND candidate_revision_id IS NULL", '3e1f31410e37f23e18ab7bfe3dc13ae6cc92843c8d3f0b22174629954b677c81', 2],
  ['lawApplicabilityCurrent', "SELECT 1 FROM legal_law_applicability WHERE id=? AND version=? AND state='pending' AND NOT iam_same_person(created_by,?,company_id)", 'a3a15a9580fc492ff51ce0e6731a14f3797a3a955829ff53606e0285fc055bd9', 3],
  ['lawSourceCheckUnchanged', "SELECT 1 FROM legal_law_source_checks WHERE source_id=? AND id<>? AND checksum=? AND state IN ('unchanged','pending_review','reviewed') LIMIT 1", '6ab871b92f5d8821489fdc0a271747595eeaa6cbedce8afbda6c37787e773847', 3],
  ['lawReviewCurrent(law,false)', 'EXISTS(SELECT 1 FROM legal_law_revisions WHERE id=? AND version=? AND state=? AND NOT iam_same_person(created_by,?,company_id))', 'f76b185c9ba959fe9ee2409da4ad471ff1987ce7498458480e71dae1abdd5f19', 4],
  ['lawReviewCurrent(law,true)', 'EXISTS(SELECT 1 FROM legal_law_revisions WHERE id=? AND version=? AND state=? )', '9a5a0be883c02b843749b0413757001e131f7ce7cfe137063a27433ccfd6cd7f', 3],
  ['lawReviewCurrent(doc,false)', 'EXISTS(SELECT 1 FROM legal_text_reviews WHERE id=? AND version=? AND state=? AND NOT iam_same_person(created_by,?,company_id))', 'c567a719bd8afd3120d73ea3609a58c3f98e7b56f858e9421bb1e92ea3797676', 4],
  ['lawReviewCurrent(doc,true)', 'EXISTS(SELECT 1 FROM legal_text_reviews WHERE id=? AND version=? AND state=? )', '291993223f34fe91d932fa07c7b106a39a7e15768acb79406092c4b30cfa5f03', 3],
  ['counselConflictCleared', "SELECT 1 FROM legal_counsel_entries WHERE item_id=? AND data->>'entryType'='conflict' AND state IN ('approved','closed')", '94e27b6e008fc19c6db0c27fb650d5a386015e3ff4c521a1acf6d59afc0993d6', 1],
  ['counselApprovedFees', "SELECT coalesce(sum(coalesce(nullif(data->>'amount',''),'0')::bigint),0) amount FROM legal_counsel_entries WHERE item_id=? AND id<>? AND state IN ('approved','billed','closed') AND data->>'entryType' IN ('fee','invoice')", 'f66b63bc8bec6d999368b8a35155185cdfdeee37009723ac65913cef6ae58cc6', 2],
] as Array<[string, string, string, number]>;

const ACTUAL: Record<string, string> = {
  lawSourceCheckPending: legalLawSourceCheckPendingSql(),
  lawSourceCurrent: legalLawSourceCurrentSql(),
  lawSourceCheckCurrent: legalLawSourceCheckCurrentSql(),
  lawApplicabilityCurrent: legalLawApplicabilityCurrentSql(),
  lawSourceCheckUnchanged: legalLawSourceCheckUnchangedSql(),
  'lawReviewCurrent(law,false)': `EXISTS(${legalLawReviewCurrentSql('legal_law_revisions', false)})`,
  'lawReviewCurrent(law,true)': `EXISTS(${legalLawReviewCurrentSql('legal_law_revisions', true)})`,
  'lawReviewCurrent(doc,false)': `EXISTS(${legalLawReviewCurrentSql('legal_text_reviews', false)})`,
  'lawReviewCurrent(doc,true)': `EXISTS(${legalLawReviewCurrentSql('legal_text_reviews', true)})`,
  counselConflictCleared: legalCounselConflictClearedSql(),
  counselApprovedFees: legalCounselApprovedFeesSql(),
};

describe('legal read-tail r56 law point-guard and counsel-entry dedupe', () => {
  it('renders all 11 replacements byte-identically to the HEAD oracles', () => {
    expect(ORIGINALS).toHaveLength(11);
    for (const [name, value, digest] of ORIGINALS) {
      expect(ACTUAL[name], name).toBe(value);
      expect(sha256(ACTUAL[name]), name).toBe(digest);
    }
  });

  it('preserves every bind count and bind order in the rendered heads', () => {
    for (const [name, , , binds] of ORIGINALS) {
      expect((ACTUAL[name].match(/\?/g) ?? []).length, name).toBe(binds);
    }
    // Bind order is pinned by the full-byte oracle; the two near-identical law
    // source heads keep their distinct company_id/rule_id placeholder order.
    expect(ACTUAL.lawSourceCurrent).toContain('id=? AND company_id=? AND rule_id=?');
    expect(ACTUAL.lawSourceCheckUnchanged).toContain('source_id=? AND id<>? AND checksum=?');
    expect(ACTUAL.counselApprovedFees).toContain('item_id=? AND id<>?');
  });

  it('adds one SELECT-free source that delegates to the verified r32/r36 heads', async () => {
    const [r56, r32, r36] = await Promise.all([
      src('legal-read-tail-r56-sources.ts'),
      src('legal-read-tail-r32-sources.ts'),
      src('legal-read-tail-r36-sources.ts'),
    ]);
    expect(r56.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(r56).toContain("from './legal-read-tail-r32-sources'");
    expect(r56).toContain("from './legal-read-tail-r36-sources'");
    expect(r32).toContain("legalPointByKeySql(columns, table, 'id')");
    expect(r36).toContain('`SELECT ${columns} FROM ${table} WHERE ${key}=?`');
    const names = [
      'legalLawSourceCheckPendingSql', 'legalLawSourceCurrentSql', 'legalLawSourceCheckCurrentSql',
      'legalLawApplicabilityCurrentSql', 'legalLawReviewCurrentSql', 'legalLawSourceCheckUnchangedSql',
      'legalCounselConflictClearedSql', 'legalCounselApprovedFeesSql',
    ];
    for (const name of names) expect(r56.match(new RegExp(`export const ${name}`, 'g')) ?? []).toHaveLength(1);
  });

  it('removes every inline holder from the four call sites', async () => {
    const [corpus, worker, features, portal] = await Promise.all([
      src('legal/law-corpus.ts'), src('legal/law-source-worker.ts'),
      src('legal/features.ts'), src('portal-purchases.ts'),
    ]);
    const held: Array<[string, string]> = [
      [corpus, ORIGINALS[0][1]], [corpus, ORIGINALS[1][1]], [corpus, ORIGINALS[2][1]],
      [corpus, ORIGINALS[3][1]], [worker, ORIGINALS[4][1]],
      [features, ORIGINALS[9][1]], [features, ORIGINALS[10][1]],
    ];
    for (const [file, literal] of held) expect(file).not.toContain(literal);
    expect(corpus).not.toContain('EXISTS(SELECT 1 FROM ${reviewTables[reviewKind].table}');
    expect(corpus).toContain('legalLawSourceCheckPendingSql()');
    expect(corpus).toContain('`EXISTS(${legalLawSourceCurrentSql()})`');
    expect(corpus).toContain('`EXISTS(${legalLawSourceCheckCurrentSql()})`');
    expect(corpus).toContain('`EXISTS(${legalLawApplicabilityCurrentSql()})`');
    expect(corpus).toContain('`EXISTS(${legalLawReviewCurrentSql(reviewTables[reviewKind].table,retiring)})`');
    expect(worker).toContain('legalLawSourceCheckUnchangedSql()');
    expect(features).toContain('legalCounselConflictClearedSql()');
    expect(features).toContain('legalCounselApprovedFeesSql()');
  });

  it('keeps the portal billing subquery inline (no clean closed head without a new SELECT)', async () => {
    const portal = await src('portal-purchases.ts');
    // Left intact on purpose: the r25 jsonb-agg head renders a closed subquery
    // and cannot carry this site's extra `AND entry.created_by=?` without
    // rewriting a shared head, so folding it would add a SELECT token (net 0).
    expect(portal).toContain("'billing',(SELECT jsonb_agg(to_jsonb(entry) ORDER BY entry.id) FROM legal_counsel_entries entry WHERE entry.item_id=i.id AND entry.created_by=?)");
  });

  it('leaves advisory, FOR UPDATE OF, LOCK TABLE, command_receipts and quoted projections intact', async () => {
    const [r9, destructionWorker, retention, purchases, closedProjection] = await Promise.all([
      src('legal-read-tail-r9-sources.ts'), src('legal/destruction-worker.ts'),
      src('retention-execution.ts'), src('purchases.ts'), src('closed-projection-source.ts'),
    ]);
    expect(r9).toContain('legalFilesAdvisoryLockSql');
    expect(r9).toContain('legalCompanyAdvisoryLockSql');
    expect(destructionWorker).toContain('FOR UPDATE OF x,i');
    expect(retention).toContain('LOCK TABLE legal_settings,legal_retention_execution_authorizations');
    expect(purchases).toContain('command_receipts');
    expect(closedProjection).toContain('cols.map((c) => `"${c}"`).join(sep)');
  });

  it('retirement:ts-statements is BEFORE=153 / AFTER=145 (r57 lowers to 142)', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(153);
    expect(total).toBeLessThanOrEqual(145);
  });
});
