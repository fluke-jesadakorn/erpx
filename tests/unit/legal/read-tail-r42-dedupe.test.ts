import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalFeatureSearchSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r42-sources';
import { legalItemSelectSql } from '../../../packages/server-legal/src/adapters/legal-read-tail-r16-sources';
import featureContract from '../../../packages/server-legal/src/adapters/legal-read-tail-r42-sources.json' with {
  type: 'json',
};
import itemContract from '../../../packages/server-legal/src/adapters/legal-read-tail-r16-sources.json' with {
  type: 'json',
};

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

const TABLE = 'legal_obligations';
const WHERE = `WHERE item_id=? AND (?='' OR strpos(lower(title||' '||data::text),lower(?))>0)`;
const TAIL = ' ORDER BY updated_at DESC,id LIMIT 50 OFFSET ?';

const members = [
  {
    file: 'legal/features.ts',
    call: 'legalFeatureSearchSql("count(*) n",featureSpecs[facts.capability].table)',
    projection: 'count(*) n',
    tail: '',
    oracle: `SELECT count(*) n FROM ${TABLE} ${WHERE}`,
  },
  {
    file: 'legal/features.ts',
    call: 'legalFeatureSearchSql("*",featureSpecs[facts.capability].table)',
    projection: '*',
    tail: TAIL,
    oracle: `SELECT * FROM ${TABLE} ${WHERE}${TAIL}`,
  },
];

describe('legal read-tail r42 parameterized feature-search head family', () => {
  it('renders byte-identical SQL for every member', () => {
    for (const m of members) expect(legalFeatureSearchSql(m.projection, TABLE) + m.tail).toBe(m.oracle);
    expect(legalFeatureSearchSql('$&', 'legal_$1')).toBe(
      "SELECT $& FROM legal_$1 WHERE item_id=? AND (?='' OR strpos(lower(title||' '||data::text),lower(?))>0)",
    );
    expect(legalItemSelectSql).toBe('SELECT i.* FROM legal_items i WHERE ');
    expect(legalItemSelectSql.endsWith(' ')).toBe(true);
  });

  it('keeps the head read-only (no writes or row locks)', () => {
    const all = members.map(m => legalFeatureSearchSql(m.projection, TABLE)).join(' ');
    expect(all).toMatch(/SELECT \S+ FROM legal_obligations/);
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('keeps one definition and removes every inline head from the caller', async () => {
    const shared = await src('legal-read-tail-r42-sources.ts');
    expect(shared.match(/export const legalFeatureSearchSql/g) ?? []).toHaveLength(1);
    const text = await src('legal/features.ts');
    expect(text).toContain('legalFeatureSearchSql(');
    expect(text.match(/legalFeatureSearchSql\(/g) ?? []).toHaveLength(2);
    expect(text).not.toContain(`FROM \${featureSpecs[facts.capability].table} ${WHERE}`);
    for (const m of members) expect(text).toContain(m.call);
  });

  it('keeps both heads in their native owner contracts and out of TypeScript', async () => {
    const [r16, r42, rust16, rust42] = await Promise.all([
      src('legal-read-tail-r16-sources.ts'),
      src('legal-read-tail-r42-sources.ts'),
      src('legal_read_tail_r16.rs'),
      src('legal_read_tail_r42.rs'),
    ]);
    expect(r16.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(r42.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(r16).toContain('legal-read-tail-r16-sources.json');
    expect(r42).toContain('legal-read-tail-r42-sources.json');
    expect(itemContract).toEqual({ itemSelect: 'SELECT i.* FROM legal_items i WHERE ' });
    expect(featureContract).toEqual({
      featureSearch:
        "SELECT {projection} FROM {table} WHERE item_id=? AND (?='' OR strpos(lower(title||' '||data::text),lower(?))>0)",
    });
    expect(rust16).toContain('include_str!("legal-read-tail-r16-sources.json")');
    expect(rust16).toContain('532f4f3fb14cd17c7c4c3a02ec7c118c21888aa4bb65795cedf6a734d1440ac7');
    expect(rust42).toContain('include_str!("legal-read-tail-r42-sources.json")');
    expect(rust42).toContain('render_legal_feature_search');
    expect(rust42).toContain('d06959eeb15ff385058b086a2c284c22e64d1119c77664f019763c014773d500');
  });

  it('leaves the corpus/citation/lock-bearing reads intact', async () => {
    const [corpus, citations, r9, destructionWorker] = await Promise.all([
      src('legal/law-corpus.ts'), src('legal/citations.ts'),
      src('legal-read-tail-r9-sources.ts'), src('legal/destruction-worker.ts'),
    ]);
    expect(corpus).toContain("nativeLegalAuxiliaryRead<LawCorpusView>(a,'laws',ruleId)");
    expect(citations).toContain('export function legalCitation');
    expect(r9).toContain('legalFilesAdvisoryLockSql');
    expect(r9).toContain('legalCompanyAdvisoryLockSql');
    expect(destructionWorker).toContain('FOR UPDATE OF x,i');
  });

  it('leaves r9/r15/r16/r18-r41 helpers untouched', async () => {
    const names = [
      'legal-read-tail-r9-sources.ts', 'legal-read-tail-r15-sources.ts', 'legal-read-tail-r16-sources.ts',
      'legal-read-tail-r18-sources.ts', 'legal-read-tail-r19-sources.ts', 'legal-read-tail-r20-sources.ts',
      'legal-read-tail-r21-sources.ts', 'legal-read-tail-r22-sources.ts', 'legal-read-tail-r23-sources.ts',
      'legal-read-tail-r24-sources.ts', 'legal-read-tail-r25-sources.ts', 'legal-read-tail-r26-sources.ts',
      'legal-read-tail-r27-sources.ts', 'legal-read-tail-r28-sources.ts', 'legal-read-tail-r29-sources.ts',
      'legal-read-tail-r30-sources.ts', 'legal-read-tail-r31-sources.ts', 'legal-read-tail-r32-sources.ts',
      'legal-read-tail-r33-sources.ts', 'legal-read-tail-r34-sources.ts', 'legal-read-tail-r35-sources.ts',
      'legal-read-tail-r36-sources.ts', 'legal-read-tail-r38-sources.ts', 'legal-read-tail-r39-sources.ts',
      'legal-read-tail-r40-sources.ts', 'legal-read-tail-r41-sources.ts',
    ];
    const texts = await Promise.all(names.map(src));
    const symbols = [
      'legalSettingsEnabledSql', 'legalManagerAvailableSql', 'legalDestructionActiveSql',
      'legalDocumentsItemPairSql', 'legalDocumentByIdSql', 'legalMembersAggSql',
      'legalFilesAdvisoryLockSql', 'legalCompanyAdvisoryLockSql',
      'legalLaterApprovalGuardSql', 'legalItemSelectSql',
      'legalRetentionPolicySelectSql', 'legalActorScopeProjectionSql', 'legalEventsByItemSql',
      'legalRateCountPrefix', 'legalDisposedDocumentPrefix', 'legalPurchaseClockCteSql',
      'legalApprovedCycleSql', 'legalHoldActiveSql', 'legalHoldTargetGuardPrefix',
      'legalMemberEditExistsSql', 'legalJsonbAggByItemSql', 'legalVersionExistsSql',
      'legalByIdAndItemSql', 'legalByIdAndCompanySql', 'legalPurchaseClockExistsSql',
      'legalOrderedHistorySql', 'legalRecentItemListSql', 'legalPointByIdSql',
      'legalStatePointSql', 'legalPointByIdAndKeySql', 'legalItemOrderedSql', 'legalPointByKeySql',
      'legalReleasedItemGuardSql', 'legalMemberEditPointSql', 'legalFactDataSql', 'legalMetricCountPrefix',
      'legalDisposedGuardSql', 'legalHoldOwnTargetSql', 'legalExistsPointGuardHeadSql',
      'legalShareDocumentReadHead',
    ];
    for (const symbol of symbols) expect(texts.join('\n')).toContain(`export const ${symbol}`);
  });

  it('retirement:ts-statements stays strictly below the recorded BEFORE=217', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(217);
  });

  it('retirement:ts-statements is BEFORE=101 -> AFTER=99 for the r16/r42 native contracts', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(101);
    expect(total).toBe(52);
  });
});
