import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  legalReleasedItemGuardSql,
  legalMemberEditPointSql,
  legalFactDataSql,
  legalMetricCountPrefix,
} from '../../../packages/server-legal/src/adapters/legal-read-tail-r38-sources';
import contract from '../../../packages/server-legal/src/adapters/legal-read-tail-r38-sources.json' with {
  type: 'json',
};

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

describe('legal read-tail r38 fragment families', () => {
  it('renders byte-identical SQL for every member of each family', () => {
    expect(legalReleasedItemGuardSql('company_id')).toBe('SELECT 1 FROM legal_items i WHERE i.id=? AND i.company_id=?');
    expect(legalReleasedItemGuardSql('version')).toBe('SELECT 1 FROM legal_items i WHERE i.id=? AND i.version=?');
    expect(legalMemberEditPointSql('?')).toBe("SELECT 1 FROM legal_members WHERE item_id=? AND user_id=? AND access='edit'");
    expect(legalMemberEditPointSql('legal_file_source.id')).toBe("SELECT 1 FROM legal_members WHERE item_id=legal_file_source.id AND user_id=? AND access='edit'");
    expect(legalFactDataSql('legal_requests', '?')).toBe('SELECT data FROM legal_requests WHERE item_id=?');
    expect(legalFactDataSql('legal_matters', 'i.id')).toBe('SELECT data FROM legal_matters WHERE item_id=i.id');
    expect(legalMetricCountPrefix('s.state', 's.id')).toBe('SELECT s.state label,count(DISTINCT s.id)::text value');
    expect(legalMetricCountPrefix('h.state', 'h.id')).toBe('SELECT h.state label,count(DISTINCT h.id)::text value');
    expect(legalFactDataSql('legal_contracts', '$&')).toBe('SELECT data FROM legal_contracts WHERE item_id=$&');
    expect(legalMetricCountPrefix('$&', '$1')).toBe('SELECT $& label,count(DISTINCT $1)::text value');
  });

  it('keeps every fragment read-only (no writes or row locks)', () => {
    const all = [
      legalReleasedItemGuardSql('company_id'), legalReleasedItemGuardSql('version'),
      legalMemberEditPointSql('?'), legalMemberEditPointSql('legal_file_source.id'),
      legalFactDataSql('legal_contracts', '?'), legalFactDataSql('legal_matters', 'i.id'),
      legalMetricCountPrefix('s.state', 's.id'), legalMetricCountPrefix('h.state', 'h.id'),
    ].join(' ');
    expect(all).toMatch(/SELECT /);
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one definition and removes every inline family literal from the callers', async () => {
    const [shared, purchases, access, attachmentPurchases, metric] = await Promise.all([
      src('legal-read-tail-r38-sources.ts'),
      src('purchases.ts'),
      src('legal/access.ts'),
      src('attachment-purchases.ts'),
      src('executive-metric-projections.ts'),
    ]);
    for (const symbol of ['legalReleasedItemGuardSql', 'legalMemberEditPointSql', 'legalFactDataSql', 'legalMetricCountPrefix']) {
      expect(shared.match(new RegExp(`export const ${symbol}`, 'g')) ?? []).toHaveLength(1);
    }
    expect(shared.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(shared).toContain('legal-read-tail-r38-sources.json');
    expect(shared).toContain('LEGAL_READ_TAIL_R38_CONTRACT_INVALID');
    expect(contract).toEqual({
      factData: 'SELECT data FROM {table} WHERE item_id={ref}',
      metricCountPrefix: 'SELECT {label} label,count(DISTINCT {id})::text value',
    });
    const rust = await src('legal_read_tail_r38.rs');
    expect(rust).toContain('include_str!("legal-read-tail-r38-sources.json")');
    expect(rust).toContain('render_legal_fact_data');
    expect(rust).toContain('render_legal_metric_count_prefix');
    expect(rust).toContain('538df0300bdb6b8e92b9db02530fe0effa6a5ce97f1951468862f55489199b1e');
    expect(purchases).toContain("legalReleasedItemGuardSql('company_id')");
    expect(purchases).toContain("legalFactDataSql(table,'i.id')");
    expect(access).toContain("legalReleasedItemGuardSql('version')");
    expect(access).toContain("legalMemberEditPointSql('?')");
    expect(access).toContain("legalFactDataSql(tables[kind],'?')");
    expect(attachmentPurchases).toContain("legalMemberEditPointSql('legal_file_source.id')");
    expect(metric).toContain("legalMetricCountPrefix(label,'s.id')");
    expect(metric).toContain("legalMetricCountPrefix(label,'h.id')");
    expect(purchases).not.toContain('i.company_id=? AND ${legalReleasePredicate(actor)}');
    expect(access).not.toContain("SELECT 1 FROM legal_members WHERE item_id=? AND user_id=? AND access='edit'");
    expect(access).not.toContain('i.version=? AND ${v.sql}');
    expect(attachmentPurchases).not.toContain('SELECT 1 FROM legal_members WHERE item_id=legal_file_source.id');
    expect(metric).not.toContain('SELECT ${label} label,count(DISTINCT');
  });

  it('leaves the corpus/citation/lock-bearing reads intact', async () => {
    const [corpus, citations, access, destructionWorker] = await Promise.all([
      src('legal/law-corpus.ts'), src('legal/citations.ts'), src('legal/access.ts'), src('legal/destruction-worker.ts'),
    ]);
    expect(corpus).toContain("nativeLegalAuxiliaryRead<LawCorpusView>(a,'laws',ruleId)");
    expect(citations).toContain('export function legalCitation');
    expect(access).toContain('legalCompanyAdvisoryLockSql');
    expect(destructionWorker).toContain('FOR UPDATE OF x,i');
  });

  it('leaves r9/r15/r16/r18-r37 families untouched', async () => {
    const names = [
      'legal-read-tail-r9-sources.ts',
      'legal-read-tail-r15-sources.ts',
      'legal-read-tail-r16-sources.ts',
      'legal-read-tail-r18-sources.ts',
      'legal-read-tail-r19-sources.ts',
      'legal-read-tail-r20-sources.ts',
      'legal-read-tail-r21-sources.ts',
      'legal-read-tail-r22-sources.ts',
      'legal-read-tail-r23-sources.ts',
      'legal-read-tail-r24-sources.ts',
      'legal-read-tail-r25-sources.ts',
      'legal-read-tail-r26-sources.ts',
      'legal-read-tail-r27-sources.ts',
      'legal-read-tail-r28-sources.ts',
      'legal-read-tail-r29-sources.ts',
      'legal-read-tail-r30-sources.ts',
      'legal-read-tail-r31-sources.ts',
      'legal-read-tail-r32-sources.ts',
      'legal-read-tail-r33-sources.ts',
      'legal-read-tail-r34-sources.ts',
      'legal-read-tail-r35-sources.ts',
      'legal-read-tail-r36-sources.ts',
    ];
    const texts = await Promise.all(names.map(src));
    const symbols = [
      'legalSettingsEnabledSql', 'legalManagerAvailableSql', 'legalDestructionActiveSql',
      'legalDocumentsItemPairSql', 'legalDocumentByIdSql', 'legalMembersAggSql',
      'legalFilesAdvisoryLockSql', 'legalCompanyAdvisoryLockSql',
      'legalLaterApprovalGuardSql',
      'legalItemSelectSql',
      'legalRetentionPolicySelectSql', 'legalActorScopeProjectionSql', 'legalEventsByItemSql',
      'legalRateCountPrefix',
      'legalDisposedDocumentPrefix',
      'legalPurchaseClockCteSql',
      'legalApprovedCycleSql',
      'legalHoldActiveSql',
      'legalHoldTargetGuardPrefix',
      'legalMemberEditExistsSql', 'legalJsonbAggByItemSql',
      'legalVersionExistsSql',
      'legalByIdAndItemSql',
      'legalByIdAndCompanySql',
      'legalPurchaseClockExistsSql',
      'legalOrderedHistorySql',
      'legalRecentItemListSql',
      'legalPointByIdSql',
      'legalStatePointSql',
      'legalPointByIdAndKeySql',
      'legalItemOrderedSql',
      'legalPointByKeySql',
    ];
    for (const symbol of symbols) {
      expect(texts.join('\n')).toContain(`export const ${symbol}`);
    }
  });

  it('retirement:ts-statements stays strictly below the recorded BEFORE=230', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(230);
    expect(total).toBeLessThanOrEqual(226);
  });

  it('retirement:ts-statements is BEFORE=105 -> AFTER=103 for the r38 native contract', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(105);
    expect(total).toBe(76);
  });
});
