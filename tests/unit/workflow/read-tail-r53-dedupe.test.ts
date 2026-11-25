import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { selectHeadSql } from '../../../packages/server-workflow/src/adapters/closed-projection-sources';
import {
  jsonEachSql,
  jsonEachApproverPoolSql,
  approverUnionProjectionSql,
} from '../../../packages/server-workflow/src/adapters/json-each-sources';
import { approvalStepByOrdinalSql } from '../../../packages/server-workflow/src/adapters/approval-projection-sources';

const sha = (s: string) => createHash('sha256').update(s).digest('hex');
const srcRoot = resolve(__dirname, '../../../packages/server-workflow/src');
const src = (rel: string) => readFile(resolve(srcRoot, rel), 'utf8');
type Oracle = readonly [digest: string, length: number, binds: number];
const check = (sql: string, [digest, length, binds]: Oracle) => {
  expect(sha(sql)).toBe(digest);
  expect(sql.length).toBe(length);
  expect((sql.match(/\?/g) ?? []).length).toBe(binds);
};

// Byte oracles captured from HEAD d107e637ea1c3b0173d308fcfa1d38a0701aa54a
// before the r53 fold. The union renderer keeps its exact emitted SQL for both
// variants; only its shared bare head moved into selectHeadSql.
const folded: Record<string, Oracle> = {
  unionStep: ['4c09727eea0919c54ccf999dd7212523b041d9de4410a7712ae92382872eacf4', 113, 0],
  unionPlain: ['f999825bd2020c65d6f9ec51a144e61e83859272c7c435ec9b4a459de38f709c', 97, 0],
};
const untouched: Record<string, Oracle> = {
  head: ['976e3c585c9435e5846708abb0cdf6cb9cb4ce733ddfe7924fd29b6e43141ddb', 7, 0],
  jsonEach: ['4c5e4225e21b00864c9de586ecf3e5dd59efb4a4b4d07055cfed39904aa9ed19', 67, 0],
  jsonEachPool: ['7669a54f69f0be1c24e7f6520f3e0fa7b07aca9c3d9572e83df3472df465d7d6', 71, 0],
  ordinalStar: ['486aaec0293b28eb016fb3a8901ba98c001005bfb64ad76d8e195aa3b4297d1c', 62, 2],
};

describe('read-tail r53 approver-union bare-head fold', () => {
  it('renders both approver-union variants byte-identically to HEAD', () => {
    check(approverUnionProjectionSql('step'), folded.unionStep!);
    check(approverUnionProjectionSql('plain'), folded.unionPlain!);
    expect(approverUnionProjectionSql('step')).toContain('UNION SELECT value FROM erpx_json_each(');
    expect(approverUnionProjectionSql('plain')).toContain('UNION SELECT value FROM erpx_json_each(');
  });

  it('leaves adjacent read tails byte-identical (no accidental over-fold)', () => {
    check(selectHeadSql, untouched.head!);
    check(jsonEachSql("COALESCE(approver_pool_json,'[]')"), untouched.jsonEach!);
    check(jsonEachApproverPoolSql('ast.approver_pool_json'), untouched.jsonEachPool!);
    check(approvalStepByOrdinalSql('*'), untouched.ordinalStar!);
  });

  it('owns one bare head in the shared renderer and delegates the union caller', async () => {
    const [union, shared] = await Promise.all([
      src('adapters/json-each-sources.ts'),
      src('adapters/closed-projection-sources.ts'),
    ]);
    expect(selectHeadSql).toBe('SELECT ');
    expect(shared.match(/export const selectHeadSql =/g) ?? []).toHaveLength(1);
    expect(shared.match(/'SELECT '/g) ?? []).toHaveLength(1);
    expect(union).toContain("import { selectHeadSql } from './closed-projection-sources'");
    expect(union.match(/`\$\{selectHeadSql\}\$\{variant/g) ?? []).toHaveLength(1);
    expect(union.match(/`SELECT \$\{variant/g) ?? []).toHaveLength(0);
    // Only jsonEachSql keeps its distinct value head; the union contributes none.
    expect(union.match(/SELECT /g) ?? []).toHaveLength(1);
  });

  it('folds only a read-only head: no writes, locks, native kinds or cross-owner reads', async () => {
    const union = await src('adapters/json-each-sources.ts');
    const forbidden = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|nativeWorkflow|@erpx\/server-/i;
    expect(union).not.toMatch(forbidden);
    // The flow-event history head stays in the native/cross-owner boundary file, untouched.
    const flow = await src('adapters/flow-planning-ports.ts');
    expect(flow.match(/`SELECT \$\{columns\}/g) ?? []).toHaveLength(1);
    for (const sql of [
      approverUnionProjectionSql('step'),
      approverUnionProjectionSql('plain'),
      jsonEachSql("COALESCE(approver_pool_json,'[]')"),
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
    expect(total).toBeLessThan(29);
    expect(total).toBe(28);
  });
});
