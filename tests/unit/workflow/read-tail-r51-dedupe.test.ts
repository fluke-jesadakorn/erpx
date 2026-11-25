import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  selectHeadSql,
  flowRoutesClosedProjectionSql,
  commandReceiptsProjectionSql,
  tableExistsGuardSql,
} from '../../../packages/server-workflow/src/adapters/closed-projection-sources';
import {
  approvalInstancesStarWhereSql,
  approvalStepsStarSql,
} from '../../../packages/server-workflow/src/adapters/approval-projection-sources';
import {
  jsonEachSql,
  jsonEachApproverPoolSql,
  approverUnionProjectionSql,
} from '../../../packages/server-workflow/src/adapters/json-each-sources';

const sha = (s: string) => createHash('sha256').update(s).digest('hex');
const srcRoot = resolve(__dirname, '../../../packages/server-workflow/src');
const src = (rel: string) => readFile(resolve(srcRoot, rel), 'utf8');
type Oracle = readonly [digest: string, length: number, binds: number];
const check = (sql: string, [digest, length, binds]: Oracle) => {
  expect(sha(sql)).toBe(digest);
  expect(sql.length).toBe(length);
  expect((sql.match(/\?/g) ?? []).length).toBe(binds);
};

// Byte oracles captured from HEAD a348ca7b before the r51 fold. The two closed
// projection renderers keep their exact emitted SQL; only their shared bare head
// moved into one owner-local renderer.
const folded: Record<string, Oracle> = {
  routesSpaced: ['c62584511a7e56e6842d2f47470ac9d38f805b16929567ac0ffffa0275a734cb', 117, 0],
  routesTight: ['3740b564fb8d7c87d2b113520cecf7ecbe7009c7bfd8a19d47921a85485ddc42', 111, 0],
  receiptsDefault: ['16e26befaaa106c17df5ac215e85583534cd95c2c3cbe0dfba113fbeea9b0ff4', 67, 0],
  receiptsTailed: ['0b38daf3e38c5d4c250bb4814d964f4e1e77be2630e5f1d7bf7cd59b76ceae14', 68, 1],
};
const untouched: Record<string, Oracle> = {
  existsGuard: ['662fca7886ad02c9902737bbc045a4bb67e12d8d1850c6335d9eb43a4a81adab', 93, 4],
  instancesStar: ['713c174272c475236e4401bb8aa09fc19804f1c9ea2d5f5b957e5b818fec6b0e', 39, 0],
  stepsStar: ['22f3316f2549b3bc73815dfccb7e0bf81594845020f5bca761421ff25f4b60a9', 28, 0],
  jsonEach: ['4c5e4225e21b00864c9de586ecf3e5dd59efb4a4b4d07055cfed39904aa9ed19', 67, 0],
  jsonEachPool: ['7669a54f69f0be1c24e7f6520f3e0fa7b07aca9c3d9572e83df3472df465d7d6', 71, 0],
  union: ['4c09727eea0919c54ccf999dd7212523b041d9de4410a7712ae92382872eacf4', 113, 0],
};

describe('read-tail r51 bare-head fold', () => {
  it('renders both closed projections byte-identically to HEAD', () => {
    check(flowRoutesClosedProjectionSql(', '), folded.routesSpaced!);
    check(flowRoutesClosedProjectionSql(','), folded.routesTight!);
    check(commandReceiptsProjectionSql(['"command_id"', '"operation"', '"result_json"']), folded.receiptsDefault!);
    check(commandReceiptsProjectionSql(['command_id', 'operation'], ',', ' WHERE command_id=?'), folded.receiptsTailed!);
  });

  it('leaves adjacent read tails byte-identical (no accidental over-fold)', () => {
    check(tableExistsGuardSql('flow_routes', 'company_id=? AND kind=? AND record_id=? AND version=?)'), untouched.existsGuard!);
    check(approvalInstancesStarWhereSql, untouched.instancesStar!);
    check(approvalStepsStarSql, untouched.stepsStar!);
    check(jsonEachSql("COALESCE(approver_pool_json,'[]')"), untouched.jsonEach!);
    check(jsonEachApproverPoolSql('ast.approver_pool_json'), untouched.jsonEachPool!);
    check(approverUnionProjectionSql('step'), untouched.union!);
  });

  it('owns one bare head in the shared renderer and delegates both closed callers', async () => {
    const [shared, approval, flow, union] = await Promise.all([
      src('adapters/closed-projection-sources.ts'),
      src('adapters/approval-projection-sources.ts'),
      src('adapters/flow-planning-ports.ts'),
      src('adapters/json-each-sources.ts'),
    ]);
    expect(selectHeadSql).toBe('SELECT ');
    expect(shared.match(/export const selectHeadSql =/g) ?? []).toHaveLength(1);
    expect(shared.match(/'SELECT '/g) ?? []).toHaveLength(1);
    expect(shared.match(/\$\{selectHeadSql\}/g) ?? []).toHaveLength(2);
    expect(shared.match(/`SELECT \$\{/g) ?? []).toHaveLength(0);
    // r52 folded the ordinal head; r53 folded the approver-union head; r40 moved
    // the flow-event history head onto native read kinds.
    expect(approval.match(/`SELECT \$\{columns\}/g) ?? []).toHaveLength(0);
    expect(approval.match(/`\$\{selectHeadSql\}\$\{columns\}/g) ?? []).toHaveLength(1);
    expect(flow.match(/`SELECT \$\{columns\}/g) ?? []).toHaveLength(0);
    expect(flow).not.toContain('flowEventsHistorySql');
    expect(flow).toContain("'wf_native_flow_events_history'");
    expect(flow).toContain("'wf_native_flow_events_record_history'");
    expect(union.match(/`SELECT \$\{variant/g) ?? []).toHaveLength(0);
    expect(union.match(/`\$\{selectHeadSql\}\$\{variant/g) ?? []).toHaveLength(1);
    expect(union).toContain("import { selectHeadSql } from './closed-projection-sources'");
  });

  it('folds only read-only heads: no writes, locks, native kinds or cross-owner reads', async () => {
    const shared = await src('adapters/closed-projection-sources.ts');
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|nativeWorkflow/i);
    for (const sql of [
      flowRoutesClosedProjectionSql(', '),
      commandReceiptsProjectionSql(['command_id'], ',', ' WHERE command_id=?'),
    ]) {
      expect(sql).toMatch(/^SELECT /);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|nativeWorkflow/i);
    }
  });

  it('retirement:ts-statements is 25 after the r40 native flow-event cut (26->25)', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(30);
    expect(total).toBe(25);
  });
});
