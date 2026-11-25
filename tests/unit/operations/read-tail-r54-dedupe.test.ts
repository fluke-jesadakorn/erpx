import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { OPERATIONS_VIEWER_CLOCK_SQL, OPERATIONS_TASK_SCOPE_SQL, operationsViewerClockHead } from '../../../packages/server-operations/src/adapters/workforce-actor-scope-source';
import { projectTaskRecordPolicy } from '../../../packages/server-operations/src/adapters/project-task-record-policy';

const srcRoot = resolve(__dirname, '../../../packages/server-operations/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-operations/src/adapters/${name}`, import.meta.url), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');

/** Independent pre-r54 byte oracle captured from record-purchases.ts:103 before
 *  the inline EXISTS/CTE head was routed through operationsViewerClockHead. */
const viewer = 'work_projection_viewer';
const tail = `${viewer} CROSS JOIN LATERAL${OPERATIONS_TASK_SCOPE_SQL}workforce_actor_scope WHERE W AND P)`;
const retired = `EXISTS(WITH ${viewer} AS MATERIALIZED${OPERATIONS_VIEWER_CLOCK_SQL} SELECT 1 FROM ${tail}`;

describe('operations read-tail r54 viewer-clock head retirement', () => {
  it('renders the retired single-space head byte-identically through the shared renderer', () => {
    expect(sha(retired)).toBe('8f92ad8a40fa701cf51a5c895619239e9b5580a15072643b374624b050d68ba9');
    expect(operationsViewerClockHead(viewer, ' ')).toBe(`EXISTS(WITH ${viewer} AS MATERIALIZED${OPERATIONS_VIEWER_CLOCK_SQL} SELECT 1 FROM `);
    expect(`${operationsViewerClockHead(viewer, ' ')}${tail}`).toBe(retired);
    expect(sha(`${operationsViewerClockHead(viewer, ' ')}${tail}`)).toBe(sha(retired));
    // The default separator still serves the unchanged r49-r53 call sites.
    expect(operationsViewerClockHead(viewer))
      .toBe(`EXISTS(WITH ${viewer} AS MATERIALIZED${OPERATIONS_VIEWER_CLOCK_SQL}\n SELECT 1 FROM `);
    expect(operationsViewerClockHead(viewer).endsWith('\n SELECT 1 FROM ')).toBe(true);
  });

  it('preserves viewer name, clock CTE, WHERE tails and bind order in the full scope', () => {
    const work = 'WORK_WHERE', project = 'PROJECT_WHERE', social = 'SOCIAL_WHERE', condition = 'CURRENT_WHERE';
    const policy = projectTaskRecordPolicy('t.id', 't.company_id', viewer);
    const pre = `(${work}) AND (${project}) AND (${social}) AND EXISTS(WITH ${viewer} AS MATERIALIZED${OPERATIONS_VIEWER_CLOCK_SQL} SELECT 1 FROM ${viewer} CROSS JOIN LATERAL${OPERATIONS_TASK_SCOPE_SQL}workforce_actor_scope WHERE ${condition} AND ${policy})`;
    const post = `(${work}) AND (${project}) AND (${social}) AND ${operationsViewerClockHead(viewer, ' ')}${viewer} CROSS JOIN LATERAL${OPERATIONS_TASK_SCOPE_SQL}workforce_actor_scope WHERE ${condition} AND ${policy})`;
    expect(post).toBe(pre);
    expect(post.startsWith(`(${work}) AND (${project}) AND (${social}) AND EXISTS(WITH ${viewer} AS MATERIALIZED${OPERATIONS_VIEWER_CLOCK_SQL} SELECT 1 FROM `)).toBe(true);
    expect(post).toContain(`CROSS JOIN LATERAL${OPERATIONS_TASK_SCOPE_SQL}workforce_actor_scope WHERE ${condition} AND ${policy})`);
    expect(post.match(/\?/g)).toEqual(pre.match(/\?/g));
  });

  it('retires the single inline copy without changing other helpers, binds or locks', async () => {
    const [scope, purchases, evaluation, attachment, alert, native] = await Promise.all([
      src('workforce-actor-scope-source.ts'), src('record-purchases.ts'),
      src('evaluation-source-policy.ts'), src('task-attachment-purchases.ts'),
      src('work-task-alert-purchases.ts'), src('native-operations-read.ts'),
    ]);
    expect(scope.match(/export const operationsViewerClockHead/g)).toHaveLength(1);
    expect(scope).toContain("sep = '\\n '");
    expect(scope).toContain('${sep}SELECT 1 FROM ');
    expect(purchases.match(/operationsViewerClockHead\(/g)).toHaveLength(1);
    expect(purchases).toContain("${operationsViewerClockHead(viewer,' ')}${viewer} CROSS JOIN LATERAL");
    expect(purchases).not.toContain('EXISTS(WITH ${viewer} AS MATERIALIZED');
    expect(purchases).not.toContain('OPERATIONS_VIEWER_CLOCK_SQL');
    expect(purchases).toContain('args:[...work.args,...social.args,actor.userId,actor.assignmentId,...current.args]');
    // r49-r53 callers keep their one-argument, newline-space rendering.
    expect(evaluation.match(/operationsViewerClockHead\(viewer\)/g)).toHaveLength(1);
    expect(attachment.match(/operationsViewerClockHead\(viewer\)/g)).toHaveLength(1);
    expect(evaluation).not.toContain("operationsViewerClockHead(viewer, ");
    expect(attachment).not.toContain("operationsViewerClockHead(viewer, ");
    // Native-read kinds, LOCK and row-locking SQL stay untouched.
    expect(native).toContain('OPERATIONS_RD_');
    expect(purchases).toContain('nativeOperationsReadFirst');
    expect(alert).toContain('LOCK TABLE staff_tasks');
    for (const text of [scope, purchases]) expect(text).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE/i);
  });

  it('retirement:ts-statements lowers the operations SELECT KPI from 72 to 71', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBe(68);
    expect(total).toBeLessThan(72);
  });
});
