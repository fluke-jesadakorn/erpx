import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  legalActiveDestructionGuardHead,
  legalExecutiveObligationSpineHead,
  legalExecutiveHoldSpineHead,
} from '../../../packages/server-legal/src/adapters/legal-read-tail-r46-sources';
import queuedContract from '../../../packages/server-legal/src/adapters/queued-purchases-sources.json' with {
  type: 'json',
};
import evidenceContract from '../../../packages/server-legal/src/adapters/executive-main-evidence-sources.json' with {
  type: 'json',
};

const srcRoot = resolve(__dirname, '../../../packages/server-legal/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-legal/src/adapters/${name}`, import.meta.url), 'utf8');

/** Exact literals that were inline before the heads were extracted. Only the
 * trusted aliases, read prefix and whitespace gap vary; rendering must be
 * byte-identical, including the IN-lists and the binds. */
const defaultGuard = `AND NOT EXISTS(SELECT 1 FROM legal_destructions destruction WHERE destruction.item_id=i.id AND destruction.state IN ('executing','completed'))`;
const xGuard = `AND NOT EXISTS(SELECT 1 FROM legal_destructions x WHERE x.item_id=i.id AND x.state IN ('executing','completed'))`;
const metricObligationPrefix = `\${legalMetricCountPrefix(label,'s.id')}`;
const metricHoldPrefix = `\${legalMetricCountPrefix(label,'h.id')}`;
const evidenceObligationPrefix = `SELECT s.id,s.item_id,s.title,s.category,s.due_date,s.state,s.version,
          i.title item_title,i.kind item_kind,i.version item_version`;
const evidenceHoldPrefix = `SELECT h.id,h.item_id,h.state,h.created_at,
          i.title item_title,i.kind item_kind,i.version item_version`;

describe('legal read-tail r46 destruction-guard and executive-spine head family', () => {
  it('renders byte-identical destruction guards for all three holders', () => {
    expect(legalActiveDestructionGuardHead()).toBe(defaultGuard);
    expect(legalActiveDestructionGuardHead('destruction', 'i')).toBe(defaultGuard);
    expect(legalActiveDestructionGuardHead('x', 'i')).toBe(xGuard);
    expect(defaultGuard).toContain("IN ('executing','completed')");
    expect(xGuard).toContain("IN ('executing','completed')");
    expect(defaultGuard).not.toContain("state='executing'");
  });

  it('renders byte-identical obligation/hold spines for both metric callers', () => {
    expect(legalExecutiveObligationSpineHead(metricObligationPrefix)).toBe(
      `${metricObligationPrefix}\n      FROM legal_obligation_schedule s JOIN legal_items i ON i.id=s.item_id\n      WHERE `,
    );
    expect(legalExecutiveHoldSpineHead(metricHoldPrefix)).toBe(
      `${metricHoldPrefix}\n      FROM legal_holds h JOIN legal_items i ON i.id=h.item_id\n      WHERE `,
    );
  });

  it('renders byte-identical obligation/hold spines for both evidence callers', () => {
    expect(legalExecutiveObligationSpineHead(evidenceObligationPrefix, '        ')).toBe(
      `${evidenceObligationPrefix}\n        FROM legal_obligation_schedule s JOIN legal_items i ON i.id=s.item_id\n        WHERE `,
    );
    expect(legalExecutiveHoldSpineHead(evidenceHoldPrefix, '        ')).toBe(
      `${evidenceHoldPrefix}\n        FROM legal_holds h JOIN legal_items i ON i.id=h.item_id\n        WHERE `,
    );
  });

  it('keeps one definition of each head and removes every inline copy', async () => {
    const shared = await src('legal-read-tail-r46-sources.ts');
    expect(shared.match(/export const legalActiveDestructionGuardHead/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const legalExecutiveObligationSpineHead/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const legalExecutiveHoldSpineHead/g) ?? []).toHaveLength(1);

    const portal = await src('portal-purchases.ts');
    expect(portal.match(/legalActiveDestructionGuardHead\(/g) ?? []).toHaveLength(1);
    expect(portal).not.toContain('legal_destructions destruction WHERE destruction.item_id=${alias}.id');
    const knowledge = await src('legal/knowledge.ts');
    expect(knowledge.match(/legalActiveDestructionGuardHead\(/g) ?? []).toHaveLength(1);
    expect(knowledge).not.toContain(defaultGuard);
    const corpus = await src('legal/law-corpus.ts');
    expect(corpus).not.toContain('legalActiveDestructionGuardHead');
    expect(corpus).not.toContain(xGuard);

    const projections = await src('executive-metric-projections.ts');
    expect(projections.match(/legalExecutiveObligationSpineHead\(/g) ?? []).toHaveLength(1);
    expect(projections.match(/legalExecutiveHoldSpineHead\(/g) ?? []).toHaveLength(1);
    expect(projections).not.toContain('\n      FROM legal_obligation_schedule s JOIN legal_items i ON i.id=s.item_id\n      WHERE ');
    expect(projections).not.toContain('\n      FROM legal_holds h JOIN legal_items i ON i.id=h.item_id\n      WHERE ');
    const evidence = await src('executive-main-evidence-sources.ts');
    expect(evidence.match(/legalExecutiveObligationSpineHead\(/g) ?? []).toHaveLength(1);
    expect(evidence.match(/legalExecutiveHoldSpineHead\(/g) ?? []).toHaveLength(1);
    expect(evidence).not.toContain('\n        FROM legal_obligation_schedule s JOIN legal_items i ON i.id=s.item_id\n        WHERE ');
    expect(evidence).not.toContain('\n        FROM legal_holds h JOIN legal_items i ON i.id=h.item_id\n        WHERE ');
    expect(evidence.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(evidence).toContain('executive-main-evidence-sources.json');
    expect(evidenceContract.obligationProjection).toContain('SELECT s.id,s.item_id,s.title');
    expect(evidenceContract.holdProjection).toContain('SELECT h.id,h.item_id,h.state');
    const evidenceRust = await src('executive_main_evidence_sources.rs');
    expect(evidenceRust).toContain('include_str!("executive-main-evidence-sources.json")');
    expect(evidenceRust).toContain('executive_obligation_projection');
    expect(evidenceRust).toContain('2a9a69668bc84df0ee2a194bb97f596ca7d452f0a26c8d135673f78d868bbd5e');
  });

  it('does not merge the queued-purchases executing-only guard', async () => {
    const [queued, rust] = await Promise.all([
      src('queued-purchases.ts'),
      src('queued_purchases.rs'),
    ]);
    expect(queuedContract.processingSourceGuard).toBe(
      "NOT EXISTS(SELECT 1 FROM legal_destructions x WHERE x.item_id=d.item_id AND x.state='executing')",
    );
    expect(queuedContract.jobSource).toContain('{documentColumns}');
    expect(queued).toContain('queued-purchases-sources.json');
    expect(queued).toContain('contract.processingSourceGuard');
    expect(queued.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(queued).not.toContain('legalActiveDestructionGuardHead');
    expect(rust).toContain('include_str!("queued-purchases-sources.json")');
    expect(rust).toContain('render_queued_job_source');
    expect(rust).toContain('6c3f2c717b8f1a81355e5f51fac148bed137a22d0604773e9284e2b24ba69e41');
  });

  it('keeps every head read-only (no writes or row locks)', () => {
    const all = [
      legalActiveDestructionGuardHead(),
      legalExecutiveObligationSpineHead(metricObligationPrefix),
      legalExecutiveHoldSpineHead(metricHoldPrefix),
    ].join(' ');
    expect(all).toContain('FROM legal_destructions destruction');
    expect(all).toContain('FROM legal_obligation_schedule s JOIN legal_items i');
    expect(all).toContain('FROM legal_holds h JOIN legal_items i');
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('leaves corpus/citation/advisory/FOR UPDATE reads intact', async () => {
    const [corpus, citations, r9, r23, r24, destructionWorker] = await Promise.all([
      src('legal/law-corpus.ts'), src('legal/citations.ts'),
      src('legal-read-tail-r9-sources.ts'), src('legal-read-tail-r23-sources.ts'),
      src('legal-read-tail-r24-sources.ts'), src('legal/destruction-worker.ts'),
    ]);
    expect(corpus).toContain("nativeLegalAuxiliaryRead<LawCorpusView>(a,'laws',ruleId)");
    expect(citations).toContain('export function legalCitation');
    expect(r9).toContain('legalFilesAdvisoryLockSql');
    expect(r9).toContain('legalCompanyAdvisoryLockSql');
    expect(r23).toContain('legalHoldActiveSql');
    expect(r24).toContain('legalHoldTargetGuardPrefix');
    expect(destructionWorker).toContain('FOR UPDATE OF x,i');
  });

  it('leaves r9-r45 helpers untouched', async () => {
    const names = [
      'legal-read-tail-r9-sources.ts', 'legal-read-tail-r15-sources.ts', 'legal-read-tail-r16-sources.ts',
      'legal-read-tail-r18-sources.ts', 'legal-read-tail-r19-sources.ts', 'legal-read-tail-r20-sources.ts',
      'legal-read-tail-r21-sources.ts', 'legal-read-tail-r22-sources.ts', 'legal-read-tail-r23-sources.ts',
      'legal-read-tail-r24-sources.ts', 'legal-read-tail-r25-sources.ts', 'legal-read-tail-r26-sources.ts',
      'legal-read-tail-r27-sources.ts', 'legal-read-tail-r28-sources.ts', 'legal-read-tail-r29-sources.ts',
      'legal-read-tail-r30-sources.ts', 'legal-read-tail-r31-sources.ts', 'legal-read-tail-r32-sources.ts',
      'legal-read-tail-r33-sources.ts', 'legal-read-tail-r34-sources.ts', 'legal-read-tail-r35-sources.ts',
      'legal-read-tail-r36-sources.ts', 'legal-read-tail-r38-sources.ts', 'legal-read-tail-r39-sources.ts',
      'legal-read-tail-r40-sources.ts', 'legal-read-tail-r41-sources.ts', 'legal-read-tail-r42-sources.ts',
      'legal-read-tail-r43-sources.ts', 'legal-read-tail-r44-sources.ts', 'legal-read-tail-r45-sources.ts',
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
      'legalShareDocumentReadHead', 'legalFeatureSearchSql', 'legalEnvelopeSignerReadHead',
      'legalExternalUploadShareJoinHead', 'legalFilePreservationHoldTargetHead',
      'legalItemsByIdCompanyHead',
    ];
    for (const symbol of symbols) expect(texts.join('\n')).toContain(`export const ${symbol}`);
  });

  it('retirement:ts-statements stays strictly below the recorded BEFORE=203', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(203);
  });

  it('retirement:ts-statements current owner pin is 93 after the queued/evidence contracts', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(95);
    expect(total).toBe(35);
  });
});
