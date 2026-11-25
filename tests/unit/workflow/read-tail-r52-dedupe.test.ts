import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  approvalStepByOrdinalSql,
  approvalStepsStarSql,
  approvalStepsByInstanceSql,
  approvalInstancesStarWhereSql,
  approvalInstancesSql,
  approvalStepsSql,
  auditEventsSql,
} from '../../../packages/server-workflow/src/adapters/approval-projection-sources';
import {
  selectHeadSql,
  flowRoutesClosedProjectionSql,
  commandReceiptsProjectionSql,
  tableExistsGuardSql,
} from '../../../packages/server-workflow/src/adapters/closed-projection-sources';
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

// Byte oracles captured from HEAD 6d08598b3c2af7d5bbebc8cbce2503ac80c91bfb
// before the r52 fold. The ordinal point-read renderer keeps its exact emitted
// SQL for every projection; only its shared bare head moved into selectHeadSql.
const folded: Record<string, Oracle> = {
  star: ['486aaec0293b28eb016fb3a8901ba98c001005bfb64ad76d8e195aa3b4297d1c', 62, 2],
  user: ['79304a2a36a5065f6ddcc8776269d468a556948fe1043efe16e69d62ee112204', 77, 2],
  cols: ['c6a0dafa5162d108ef05ba6f17d0066e921b3b398667ecbca8e039c28063fb78', 75, 2],
};
const untouched: Record<string, Oracle> = {
  stepsStar: ['22f3316f2549b3bc73815dfccb7e0bf81594845020f5bca761421ff25f4b60a9', 28, 0],
  stepsByInstance: ['0f8d3ba9d9decb60d659b143be773bb571cb370ce8d70528e00cef770c8228ec', 65, 1],
  instancesStar: ['713c174272c475236e4401bb8aa09fc19804f1c9ea2d5f5b957e5b818fec6b0e', 39, 0],
  instances: ['0ead25b9add24af95a1dbed3c02b4765ee85c4948b1e86d9c981ccce8170fbfe', 251, 0],
  steps: ['8ef0cce32ed76153265573b847df84d734e1d1cae633c5e129899479d8ed5cd0', 161, 0],
  audits: ['b2cb1c386524f7cc7b2a821639165f1e3b1174b4aee3b1abf4044668b74e34bd', 161, 0],
  routesSpaced: ['c62584511a7e56e6842d2f47470ac9d38f805b16929567ac0ffffa0275a734cb', 117, 0],
  routesTight: ['3740b564fb8d7c87d2b113520cecf7ecbe7009c7bfd8a19d47921a85485ddc42', 111, 0],
  receipts: ['16e26befaaa106c17df5ac215e85583534cd95c2c3cbe0dfba113fbeea9b0ff4', 67, 0],
  receiptsTailed: ['0b38daf3e38c5d4c250bb4814d964f4e1e77be2630e5f1d7bf7cd59b76ceae14', 68, 1],
  existsGuard: ['79cfdd4bbd3f80239ceedb322d141bc6bad839d865526151e5917ed183a03a98', 92, 4],
  jsonEach: ['4c5e4225e21b00864c9de586ecf3e5dd59efb4a4b4d07055cfed39904aa9ed19', 67, 0],
  jsonEachPool: ['7669a54f69f0be1c24e7f6520f3e0fa7b07aca9c3d9572e83df3472df465d7d6', 71, 0],
  unionStep: ['4c09727eea0919c54ccf999dd7212523b041d9de4410a7712ae92382872eacf4', 113, 0],
  unionPlain: ['f999825bd2020c65d6f9ec51a144e61e83859272c7c435ec9b4a459de38f709c', 97, 0],
};

describe('read-tail r52 ordinal point-read bare-head fold', () => {
  it('renders every ordinal point-read projection byte-identically to HEAD', () => {
    check(approvalStepByOrdinalSql('*'), folded.star!);
    check(approvalStepByOrdinalSql('approver_user_id'), folded.user!);
    check(approvalStepByOrdinalSql('"id","ordinal"'), folded.cols!);
  });

  it('leaves adjacent read tails byte-identical (no accidental over-fold)', () => {
    check(approvalStepsStarSql, untouched.stepsStar!);
    check(approvalStepsByInstanceSql, untouched.stepsByInstance!);
    check(approvalInstancesStarWhereSql, untouched.instancesStar!);
    check(approvalInstancesSql, untouched.instances!);
    check(approvalStepsSql, untouched.steps!);
    check(auditEventsSql, untouched.audits!);
    check(flowRoutesClosedProjectionSql(', '), untouched.routesSpaced!);
    check(flowRoutesClosedProjectionSql(','), untouched.routesTight!);
    check(commandReceiptsProjectionSql(['"command_id"', '"operation"', '"result_json"']), untouched.receipts!);
    check(commandReceiptsProjectionSql(['command_id', 'operation'], ',', ' WHERE command_id=?'), untouched.receiptsTailed!);
    check(tableExistsGuardSql('flow_routes', 'company_id=? AND kind=? AND record_id=? AND version=?'), untouched.existsGuard!);
    check(jsonEachSql("COALESCE(approver_pool_json,'[]')"), untouched.jsonEach!);
    check(jsonEachApproverPoolSql('ast.approver_pool_json'), untouched.jsonEachPool!);
    check(approverUnionProjectionSql('step'), untouched.unionStep!);
    check(approverUnionProjectionSql('plain'), untouched.unionPlain!);
  });

  it('owns one bare head in the shared renderer and delegates the ordinal caller', async () => {
    const [approval, shared, flow, union] = await Promise.all([
      src('adapters/approval-projection-sources.ts'),
      src('adapters/closed-projection-sources.ts'),
      src('adapters/flow-planning-ports.ts'),
      src('adapters/json-each-sources.ts'),
    ]);
    expect(selectHeadSql).toBe('SELECT ');
    expect(shared.match(/export const selectHeadSql =/g) ?? []).toHaveLength(1);
    expect(shared.match(/'SELECT '/g) ?? []).toHaveLength(1);
    expect(approval).toContain("import { selectHeadSql } from './closed-projection-sources'");
    expect(approval.match(/`\$\{selectHeadSql\}\$\{columns\}/g) ?? []).toHaveLength(1);
    expect(approval.match(/`SELECT \$\{columns\}/g) ?? []).toHaveLength(0);
    // The flow-event history bare head remains for a later round; r53 folded the approver union.
    expect(flow.match(/`SELECT \$\{columns\}/g) ?? []).toHaveLength(1);
    expect(union.match(/`SELECT \$\{variant/g) ?? []).toHaveLength(0);
    expect(union.match(/`\$\{selectHeadSql\}\$\{variant/g) ?? []).toHaveLength(1);
    expect(union).toContain("import { selectHeadSql } from './closed-projection-sources'");
  });

  it('folds only read-only heads: no writes, locks, native kinds or cross-owner reads', async () => {
    const [approval, shared] = await Promise.all([
      src('adapters/approval-projection-sources.ts'),
      src('adapters/closed-projection-sources.ts'),
    ]);
    const forbidden = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|nativeWorkflow|@erpx\/server-/i;
    expect(approval).not.toMatch(forbidden);
    expect(shared).not.toMatch(forbidden);
    for (const sql of [
      approvalStepByOrdinalSql('*'),
      approvalStepByOrdinalSql('approver_user_id'),
      approvalStepsStarSql,
      approvalInstancesStarWhereSql,
      flowRoutesClosedProjectionSql(','),
    ]) {
      expect(sql).toMatch(/^SELECT /);
      expect(sql).not.toMatch(forbidden);
    }
  });

  it('retirement:ts-statements is 28 after the r53 head fold (29->28)', async () => {
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
    expect(total).toBe(28);
  });
});
