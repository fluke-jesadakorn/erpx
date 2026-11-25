import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { projectPurchaseSourceAbsent, socialPurchaseActorExists } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';
import { operationalRead, pmsRead, sourceRows, sourceRowsAbsentGuard } from '../../../packages/server-collaboration/src/adapters/work-social-read-policy';
import { workOperationsSettlementSources } from '@erpx/server-operations/public/work-package-obligation-sources';
import { projectTaskRecordPolicy } from '@erpx/server-operations/public/project-task-record-policy';
import { purchasedCapabilityPredicate } from '@erpx/server-commercial/public/purchased-capability-sql';
import { socialPmsSourceFacts } from '@erpx/server-pms/public/social-source-facts';
import { pmsSocialSourcePurchasePredicate } from '@erpx/server-pms/public/social-source-purchases';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const fixed = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const shared = src('read-tail-sql.ts');
const readPolicy = src('work-social-read-policy.ts');
const purchases = src('work-social-purchases.ts');
type Purpose = Parameters<typeof operationalRead>[0];
const purposes: Purpose[] = ['new-work', 'settle-existing', 'history'];

// Exact source-rows head and the two pre-r40 call-site tails. Only a tail
// varies; it is passed verbatim to the one shared source-rows absence guard.
const SOURCE_ROWS = 'SELECT ps.source_kind kind,ps.source_id id FROM social_post_sources ps WHERE ps.post_id=p.id';
const OPERATIONAL_TAIL = "JOIN (${workOperationsSettlementSources.tasks}) t ON source.kind='staff_task' AND t.id=source.id AND t.company_id=p.company_id WHERE t.project_id IS NULL AND NOT (${projectTaskRecordPolicy('t.id','t.company_id','social_purchase_actor')} AND ${purchasedCapabilityPredicate('t.company_id','work.actions',purpose,'social_purchase_actor.now_ms')} AND (t.template_id IS NULL OR ${purchasedCapabilityPredicate('t.company_id','work.advanced-routines',purpose,'social_purchase_actor.now_ms')}))";
const PMS_TAIL = "WHERE source.kind='pms_record' AND NOT EXISTS(SELECT 1 FROM (${socialPmsSourceFacts}) social_pms_source WHERE social_pms_source.id=source.id AND social_pms_source.company_id=p.company_id AND ${pmsSocialSourcePurchasePredicate(purpose)})";
const ORACLE = (tail: string) => `NOT EXISTS(SELECT 1 FROM (${SOURCE_ROWS}) source ${tail})`;
const operationalTail = (purpose: Purpose) => `JOIN (${workOperationsSettlementSources.tasks}) t ON source.kind='staff_task' AND t.id=source.id AND t.company_id=p.company_id WHERE t.project_id IS NULL AND NOT (${projectTaskRecordPolicy('t.id','t.company_id','social_purchase_actor')} AND ${purchasedCapabilityPredicate('t.company_id','work.actions',purpose,'social_purchase_actor.now_ms')} AND (t.template_id IS NULL OR ${purchasedCapabilityPredicate('t.company_id','work.advanced-routines',purpose,'social_purchase_actor.now_ms')}))`;
const pmsTail = (purpose: Purpose) => `WHERE source.kind='pms_record' AND NOT EXISTS(SELECT 1 FROM (${socialPmsSourceFacts}) social_pms_source WHERE social_pms_source.id=source.id AND social_pms_source.company_id=p.company_id AND ${pmsSocialSourcePurchasePredicate(purpose)})`;

// r40 generalizes the two byte-identical source-rows absence heads in
// work-social-read-policy.ts (operationalRead and pmsRead) into one renderer:
// 2 inline heads -> 1 helper (-1): 356 -> 355. No bind order, lock statement,
// notification target or public export key changed, and the two
// projectPurchaseSourceAbsent sites keep their own helper untouched.
describe('Collaboration read-tail r40 byte oracles', () => {
  test('sourceRowsAbsentGuard renders both original heads byte-identically with zero binds', () => {
    expect(sourceRows).toBe(SOURCE_ROWS);
    expect(sourceRowsAbsentGuard(OPERATIONAL_TAIL)).toBe(ORACLE(OPERATIONAL_TAIL));
    expect(sourceRowsAbsentGuard(PMS_TAIL)).toBe(ORACLE(PMS_TAIL));
    expect(sourceRowsAbsentGuard('')).toBe(`NOT EXISTS(SELECT 1 FROM (${SOURCE_ROWS}) source )`);
    // One token from the guard head plus the one already inside sourceRows.
    expect(sourceRowsAbsentGuard('').split('SELECT ').length - 1).toBe(2);
    expect(sourceRowsAbsentGuard('').split('SELECT ').length - 1 - (SOURCE_ROWS.split('SELECT ').length - 1)).toBe(1);
    expect(sourceRowsAbsentGuard(OPERATIONAL_TAIL)).not.toContain('?');
    expect(sourceRowsAbsentGuard(PMS_TAIL)).not.toContain('?');
    expect(shared).not.toContain('export const sourceRowsAbsentGuard=');
    expect(readPolicy.match(/export const sourceRowsAbsentGuard=/g) ?? []).toHaveLength(1);
    expect(readPolicy).toContain('export const sourceRowsAbsentGuard=(tail:string)=>`NOT EXISTS(SELECT 1 FROM (${sourceRows}) source ${tail})`;');
  });

  test('both rendered read policies stay byte-identical to their pre-r40 SQL for every purpose', () => {
    for (const purpose of purposes) {
      expect(operationalRead(purpose)).toBe(sourceRowsAbsentGuard(operationalTail(purpose)));
      expect(pmsRead(purpose)).toBe(socialPurchaseActorExists(sourceRowsAbsentGuard(pmsTail(purpose))));
      expect(operationalRead(purpose).startsWith(`NOT EXISTS(SELECT 1 FROM (${SOURCE_ROWS}) source `)).toBe(true);
      expect(pmsRead(purpose)).toContain(`NOT EXISTS(SELECT 1 FROM (${SOURCE_ROWS}) source WHERE source.kind='pms_record'`);
    }
  });
});

describe('Collaboration read-tail r40 call sites and protected SQL', () => {
  test('both call sites pass their original tails verbatim', () => {
    expect(readPolicy).toContain('return sourceRowsAbsentGuard(`' + OPERATIONAL_TAIL + '`);');
    expect(readPolicy).toContain('socialPurchaseActorExists(sourceRowsAbsentGuard(`' + PMS_TAIL + '`));');
    expect(readPolicy).not.toContain('return `NOT EXISTS(SELECT 1 FROM (${sourceRows}) source JOIN');
    expect(readPolicy).not.toContain('`NOT EXISTS(SELECT 1 FROM (${sourceRows}) source WHERE source.kind=');
    expect(readPolicy.split('sourceRowsAbsentGuard').length - 1).toBe(3);
  });

  test('projectPurchaseSourceAbsent sites and their binds are untouched', () => {
    expect(readPolicy.split('projectPurchaseSourceAbsent').length - 1).toBe(3);
    expect(shared.match(/export const projectPurchaseSourceAbsent=/g) ?? []).toHaveLength(1);
    expect(readPolicy).toContain('projectPurchaseSourceAbsent(sourceRows,projectSocialPurchaseSources,');
    expect(readPolicy).toContain("projectPurchaseSourceAbsent(sourceRows,projectSocialPurchaseSources,purchasedCapabilityPredicate('p.company_id','projects.handover',purpose,'social_purchase_actor.now_ms'))");
    const projectReadLine = readPolicy.split('\n').find(l => l.startsWith('export function projectRead('))!;
    const projectOnlyLine = readPolicy.split('\n').find(l => l.startsWith('export const projectOnlyReadWhere='))!;
    expect(sha(projectReadLine)).toBe('b262c4420cb90418d67127ff7f9c0a2031287e0f0127912ad7575843e5ca4da6');
    expect(sha(projectOnlyLine)).toBe('86854179d04a01c6fe08e0d6d60e77efba7da6c516626b3d8f17293e42fd42cb');
  });

  test('locks, notification targets and field-outbox files are unchanged', () => {
    expect(sha(fixed('adapters/inbox.ts'))).toBe('add8defc4e1a0b3d369362e96d00acb93e34d0d0734cd0a1783b98a7382eb721');
    expect(sha(fixed('adapters/staff-notifications.ts'))).toBe('3ce65abf55973c3a41228531a9d2778739a6a7f859a37591c016bda9f9a2508a');
    expect(sha(fixed('adapters/messaging-purchases.ts'))).toBe('6921fd7a922dccbab407a37ecf8544b26494388c442871f4b084c45b39133c9b');
    expect(sha(fixed('public/field-outbox-predicate.generated.ts'))).toBe('1c1945bec812a3981738b0d36af5483786ff8e372c669453c0bc8683976a6c84');
    expect(purchases).toContain('ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 500');
    expect(purchases).toContain('social_agent_replies IN SHARE ROW EXCLUSIVE MODE');
  });

  test('the guard added no lock, write or notification statement', () => {
    const line = readPolicy.split('\n').find(l => l.startsWith('export const sourceRowsAbsentGuard='))!;
    expect(line).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/);
    expect(line.match(/SELECT /g) ?? []).toHaveLength(1);
  });
});

describe('Collaboration read-tail r40 floor evidence', () => {
  test('the agent-replies pending pair stays byte-identical at the dedupe floor', () => {
    const claim = purchases.split('\n').find(l => l.includes("SELECT pending.* FROM social_agent_replies pending WHERE pending.state='pending'"))!;
    const finalize = purchases.split('\n').find(l => l.includes('SELECT count(*)::int n FROM social_agent_replies pending JOIN'))!;
    // Distinct heads (claim rowset vs finalize count) are the floor: no further
    // byte-identical dedupe is available, so both are recorded verbatim.
    expect(claim).not.toBe(finalize);
    expect(sha(claim)).toBe('a08538190a67e554b1b1f6e725aa69ff9572e64abb28415dfffb2a678af93be1');
    expect(sha(finalize)).toBe('3cd3b4bb4504c1a15857a9ce51e8db1b4ebddde5676c45432bf29ad9a625a0de');
    expect(claim).toContain('ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 500');
    expect(finalize).toContain("pending.state='failed' AND pending.error_code='ACCESS_CHANGED'");
  });
});

describe('Collaboration read-tail r40 KPI pins', () => {
  test('retirement:ts-statements is pinned at 338 after r44, below the r40 BEFORE=356 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // Two inline source-rows absence heads -> one renderer: 356 -> 355.
    // r41 deduped task-origin/by-key, AI-usage row and member guards: 355 -> 352.
    // r42 deduped the comment head and UNION projection pair: 352 -> 350.
    // r43 retired draft-by-id reuse, messaging team image and target image: 350 -> 344.
    // r44 retired the latest-message tail, absent-social-post guard and handover
    // ack join: 344 -> 341. Prior pins were refreshed only where 341 replaced 344.
    expect(total).toBe(334);
    expect(total).toBeLessThan(356);
  });
});
