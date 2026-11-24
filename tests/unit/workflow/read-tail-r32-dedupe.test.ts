import { readFile } from 'node:fs/promises';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { nativeFlowPlanningPorts } from '../../../packages/server-workflow/src/adapters/flow-planning-ports';
import { nativeApprovalPlanningPorts } from '../../../packages/server-workflow/src/adapters/approval-planning';
import { approvalInstancesStarWhereSql, approvalStepsStarSql, approvalStepsByInstanceSql } from '../../../packages/server-workflow/src/adapters/approval-projection-sources';
import { workApprovalSettlementSources } from '../../../packages/server-workflow/src/adapters/work-package-obligation-sources';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

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

const maxRevision = 'SELECT COALESCE(MAX(revision),0)';

describe('flow-defaults revision span dedupe (r32)', () => {
  it('emits byte-identical SQL for the revision read and the route-version guard', () => {
    const { db, calls } = capture();
    const repo = nativeFlowPlanningPorts(db).repository as unknown as {
      readReadFlowDefaultsRevision: (companyId: string) => unknown;
      guard25: (key: string, value: string, companyId: string, revision: number) => { values: unknown[] };
    };
    repo.readReadFlowDefaultsRevision('co-1');
    expect(calls).toEqual([{ sql: `${maxRevision} revision FROM flow_defaults WHERE company_id=?`, binds: ['co-1'] }]);

    const guard = repo.guard25('key', 'route-version', 'co-2', 7);
    expect(guard.values).toEqual(['key', 'route-version', 'co-2', 7, `(${maxRevision} FROM flow_defaults WHERE company_id=?)=?`]);
  });

  it('keeps one byte-identical span literal in source', async () => {
    const src = await readFile(new URL('../../../packages/server-workflow/src/adapters/flow-planning-ports.ts', import.meta.url), 'utf8');
    expect(src.match(/SELECT COALESCE\(MAX\(revision\),0\)/g)).toHaveLength(1);
  });
});

describe('approval-instance star-where prefix dedupe (r32)', () => {
  it('keeps one byte-identical prefix shared by all three star reads', () => {
    expect(approvalInstancesStarWhereSql).toBe('SELECT * FROM approval_instances WHERE ');
    expect(workApprovalSettlementSources.instances).toBe(`${approvalInstancesStarWhereSql}record_type='task'`);
    const { db, calls } = capture();
    const repo = nativeApprovalPlanningPorts(db).repository as unknown as {
      readCurrentApprovalEffect: (kind: string, id: string, companyId: string) => unknown;
      readRerouteApprovalInstance: (id: string, companyId: string) => unknown;
    };
    repo.readCurrentApprovalEffect('expense', 'rec-1', 'co-1');
    repo.readRerouteApprovalInstance('inst-1', 'co-2');
    expect(calls).toEqual([
      { sql: `${approvalInstancesStarWhereSql}record_type=? AND record_id=? AND company_id=? ORDER BY insertion_order DESC LIMIT 1`, binds: ['expense', 'rec-1', 'co-1'] },
      { sql: `${approvalInstancesStarWhereSql}id=? AND company_id=?`, binds: ['inst-1', 'co-2'] },
    ]);
  });

  it('keeps one byte-identical prefix literal in source', async () => {
    const src = await readFile(new URL('../../../packages/server-workflow/src/adapters/approval-projection-sources.ts', import.meta.url), 'utf8');
    expect(src.match(/SELECT \* FROM approval_instances WHERE /g)).toHaveLength(1);
  });
});

describe('approval-steps star projection span dedupe (r33)', () => {
  it('keeps one byte-identical star projection shared by both consumers', () => {
    expect(approvalStepsStarSql).toBe('SELECT * FROM approval_steps');
    expect(approvalStepsByInstanceSql).toBe(`${approvalStepsStarSql} WHERE instance_id=? ORDER BY ordinal`);
    expect(workApprovalSettlementSources.steps).toBe(approvalStepsStarSql);
  });

  it('keeps a single star-projection literal in source', async () => {
    const [projectionSrc, settlementSrc] = await Promise.all([
      readFile(new URL('../../../packages/server-workflow/src/adapters/approval-projection-sources.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../packages/server-workflow/src/adapters/work-package-obligation-sources.ts', import.meta.url), 'utf8'),
    ]);
    expect(projectionSrc.match(/SELECT \* FROM approval_steps/g)).toHaveLength(1);
    expect(settlementSrc).not.toContain('SELECT * FROM approval_steps');
  });
});
