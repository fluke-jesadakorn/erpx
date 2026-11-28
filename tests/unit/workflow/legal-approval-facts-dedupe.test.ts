import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { approvalStepByOrdinalSql } from '../../../modules/workflow/server/src/adapters/approval-projection-sources';
import { legalApprovalFacts } from '../../../modules/workflow/server/src/adapters/legal-approval-facts';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';

const pointReadSql = (columns: string) => `SELECT ${columns} FROM approval_steps WHERE instance_id=? AND ordinal=?`;

function capture() {
  const calls: { sql: string; binds: unknown[] }[] = [];
  const stmt = (sql: string) => ({
    bind(...values: unknown[]) { calls.push({ sql, binds: values }); return this; },
    first() { return undefined; },
    all() { return { results: [] as unknown[] }; },
  });
  const db = { prepare: (sql: string) => stmt(sql) } as unknown as SqlDatabase;
  return { db, calls };
}

describe('legal approval ordinal point-read generator family (r30)', () => {
  it('emits byte-identical SQL for both projections', () => {
    expect(approvalStepByOrdinalSql('*')).toBe(pointReadSql('*'));
    expect(approvalStepByOrdinalSql('approver_user_id')).toBe(pointReadSql('approver_user_id'));
  });
  it('currentStep prepares the * projection with unchanged binds', () => {
    const { db, calls } = capture();
    legalApprovalFacts(db).currentStep({ instanceId: 'inst-1', currentStep: 3 });
    expect(calls).toEqual([{ sql: pointReadSql('*'), binds: ['inst-1', 3] }]);
  });
  it('nextReviewer prepares the approver_user_id projection with unchanged binds', () => {
    const { db, calls } = capture();
    legalApprovalFacts(db).nextReviewer({ instanceId: 'inst-2', nextStep: 4 });
    expect(calls).toEqual([{ sql: pointReadSql('approver_user_id'), binds: ['inst-2', 4] }]);
  });
  it('keeps one generator definition delegating to the shared bare head (r52)', async () => {
    const { readFile } = await import('node:fs/promises');
    const sources = await readFile(new URL('../../../modules/workflow/server/src/adapters/approval-projection-sources.ts', import.meta.url), 'utf8');
    const facts = await readFile(new URL('../../../modules/workflow/server/src/adapters/legal-approval-facts.ts', import.meta.url), 'utf8');
    expect(sources.match(/`SELECT \$\{columns\} FROM approval_steps WHERE instance_id=\? AND ordinal=\?`/g) ?? []).toHaveLength(0);
    expect(sources.match(/`\$\{selectHeadSql\}\$\{columns\} FROM approval_steps WHERE instance_id=\? AND ordinal=\?`/g)).toHaveLength(1);
    expect(facts).not.toContain('FROM approval_steps WHERE instance_id=? AND ordinal=?');
    expect(facts.match(/approvalStepByOrdinalSql\(/g)).toHaveLength(2);
  });
});
