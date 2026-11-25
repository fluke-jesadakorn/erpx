import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { describe, it, expect } from 'vitest';
import { operationsJsonbImageHead } from '../../../packages/server-operations/src/adapters/read-tail-r61-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-operations/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-operations/src/adapters/${name}`, import.meta.url), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');
const binds = (sql: string) => (sql.match(/\?/g) ?? []).length;

/** Independent pre-r61 byte oracles. Both literals were inline in HEAD; the
 *  interpolations are replaced by deterministic §tokens in capture order, so
 *  the hashes below pin the exact SQL text the renderer must reproduce. */
const retired = {
  head: "SELECT jsonb_build_object('",
  taskImage: "SELECT jsonb_build_object('task',to_jsonb(t),'project',project.image,\n 'template',§1,\n 'schedule',§2,\n 'delegations',§3,\n 'social',social.image,'native_social',social.work_social)::text image\n FROM staff_tasks t LEFT JOIN (§4) project ON project.task_id=t.id AND project.company_id=t.company_id\n LEFT JOIN (§5) social ON social.id=t.id AND social.company_id=t.company_id\n WHERE t.id=? AND t.company_id=?",
  references: "SELECT jsonb_build_object('template',§1,'schedule',§2)::text image",
};
const oracle = {
  head: 'a385f93461f75d7ef79b3a29ccb3a29583cacc23ee59ac2e2febf7b7281ce555',
  taskImage: '93d4fbd2ad8bc3d4b79cb0f74f4a78c2b27a17a2426a1b651b9dc45a53ac83e7',
  references: '7b4030010af7dae98d43286f6839d2ba5f7fda7cb67110ea89a744cc63e052ac',
};

describe('operations read-tail r61 jsonb image head retirement', () => {
  it('renders the shared head byte-identically and stays read-only', () => {
    expect(operationsJsonbImageHead('x')).toBe(retired.head + 'x');
    expect(sha(retired.head)).toBe(oracle.head);
    expect(operationsJsonbImageHead('x').match(/SELECT /g)).toHaveLength(1);
    expect(binds(operationsJsonbImageHead('x'))).toBe(0);
    expect(operationsJsonbImageHead('x')).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|INSERT\s+INTO|DELETE\s+FROM/i);
  });

  it('renders both family members byte-identically through the retired oracles', () => {
    const task = operationsJsonbImageHead("task',to_jsonb(t),'project',project.image,\n 'template',§1,\n 'schedule',§2,\n 'delegations',§3,\n 'social',social.image,'native_social',social.work_social)::text image\n FROM staff_tasks t LEFT JOIN (§4) project ON project.task_id=t.id AND project.company_id=t.company_id\n LEFT JOIN (§5) social ON social.id=t.id AND social.company_id=t.company_id\n WHERE t.id=? AND t.company_id=?");
    const refs = operationsJsonbImageHead("template',§1,'schedule',§2)::text image");
    expect(task).toBe(retired.taskImage);
    expect(sha(task)).toBe(oracle.taskImage);
    expect(refs).toBe(retired.references);
    expect(sha(refs)).toBe(oracle.references);
    for (const sql of [task, refs]) {
      expect(sql.startsWith(retired.head)).toBe(true);
      expect(sql.match(/SELECT /g)).toHaveLength(1);
      expect(sql).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    expect(binds(task)).toBe(2);
    expect(binds(refs)).toBe(0);
  });

  it('keeps one head definition and routes both family files through it', async () => {
    const [helper, attachment, purchases] = await Promise.all([
      src('read-tail-r61-sources.ts'), src('task-attachment-read.ts'), src('work-task-purchases.ts'),
    ]);
    expect(helper.match(/export const operationsJsonbImageHead/g)).toHaveLength(1);
    expect(helper).toContain('`SELECT jsonb_build_object(\'${tail}`');
    expect(attachment.match(/operationsJsonbImageHead\(/g)).toHaveLength(1);
    expect(attachment).toContain("from './read-tail-r61-sources'");
    expect(attachment).toContain("operationsJsonbImageHead(`task',to_jsonb(t),'project',project.image,");
    expect(purchases.match(/operationsJsonbImageHead\(/g)).toHaveLength(1);
    expect(purchases).toContain("operationsJsonbImageHead(`template',${operationsChildJsonByBoundId(");
    for (const text of [attachment, purchases]) expect(text).not.toContain("`SELECT jsonb_build_object('");
    expect(attachment.match(/SELECT jsonb_build_object/g) ?? []).toHaveLength(0);
    expect(purchases.match(/SELECT jsonb_build_object/g) ?? []).toHaveLength(0);
  });

  it('leaves viewer clock, native reads, locks and prior r53-r60 helpers untouched', async () => {
    const [scope, native, alert, locks, r53, r55, r56, r57, r58, alias] = await Promise.all([
      src('workforce-actor-scope-source.ts'), src('native-operations-read.ts'),
      src('work-task-alert-purchases.ts'), src('project-source-locks.ts'),
      src('read-tail-r53-sources.ts'), src('read-tail-r55-sources.ts'),
      src('read-tail-r56-sources.ts'), src('read-tail-r57-sources.ts'),
      src('read-tail-r58-sources.ts'), src('operations-alias-select-all.ts'),
    ]);
    expect(scope).toContain('OPERATIONS_VIEWER_CLOCK_SQL');
    expect(native).toContain('OPERATIONS_RD_');
    expect(alert).toContain('LOCK TABLE staff_tasks');
    expect(locks).toContain('LOCK TABLE staff_tasks IN SHARE ROW EXCLUSIVE MODE');
    expect(r53).toContain('OPERATIONS_STAFF_TASK_HEAD');
    expect(r55).toContain('OPERATIONS_EXISTS_HEAD');
    expect(r56).toContain('OPERATIONS_SELECT_1_FROM');
    expect(r57).toContain('OPERATIONS_OWNER_PROJECTION_ID_HEAD');
    expect(r58).toContain('`SELECT ${projection} FROM (${source})`');
    expect(alias).toContain('`SELECT ${alias}.* FROM ${table} ${alias} WHERE ${tail}`');
  });

  it('retirement:ts-statements lowers the operations SELECT KPI from 63 to 62', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBe(62);
    expect(total).toBeLessThan(63);
  });
});
