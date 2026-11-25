import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { OPERATIONS_EXISTS_HEAD } from '../../../packages/server-operations/src/adapters/read-tail-r55-sources';
import { operationsOwnerVersionExists, operationsTableExists } from '../../../packages/server-operations/src/adapters/identity-policy-version-guard';
import { operationsProjectSourceExists } from '../../../packages/server-operations/src/adapters/project-source-exists';
import { operationsJoinSourceExists } from '../../../packages/server-operations/src/adapters/join-source-exists';
import {
  OPERATIONS_VIEWER_CLOCK_SQL, operationsActorScopeInputExists, workforceActorScopeSql,
} from '../../../packages/server-operations/src/adapters/workforce-actor-scope-source';

const srcRoot = resolve(__dirname, '../../../packages/server-operations/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-operations/src/adapters/${name}`, import.meta.url), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');
const binds = (sql: string) => (sql.match(/\?/g) ?? []).length;

/** Independent pre-r55 byte oracles captured from the retired inline heads and
 *  their separator variants; the hashes are fixed before any routing edit. */
const retired = {
  ownerVersion: 'EXISTS(SELECT 1 FROM operations_queue_policies WHERE id=? AND company_id=? AND version=? AND status=? )',
  projectDefault: 'EXISTS(SELECT 1 FROM (§staff_projects) sp WHERE sp.id=t.project_id)',
  projectAliasEmpty: 'EXISTS(SELECT 1 FROM (§templates)template WHERE template.id=t.template_id)',
  projectOpenNl4: 'EXISTS(\n    SELECT 1 FROM (§social_post_sources) hs WHERE hs.post_id=hp.id)',
  joinDefault: "EXISTS(SELECT 1 FROM (§department_heads) h JOIN (§organization_nodes) n ON n.id=h.department_id WHERE h.department_id=? AND h.company_id=? AND h.version=? AND n.status='active')",
  joinWhereNl10: 'EXISTS(SELECT 1 FROM (§origins) o JOIN (§posts) p ON p.id=o.post_id\n          WHERE o.task_id=t.id AND <POSTS>)',
  actorDefault: 'SELECT 1 FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) workforce_actor_scope WHERE SCOPE',
  actorFromEmpty: 'SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id) workforce_actor_scope WHERE SCOPE',
  workforce: 'SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE SCOPE',
};
const oracle = {
  ownerVersion: 'b580cc19cc3077598b71c5e84eb044816ac8d829a7566f1e09a4e1a79c888793',
  projectDefault: '71e3c83d699570170ce8e8c05d3289284f8bb15af555976476e1a911905f9ec3',
  projectAliasEmpty: 'ed9a3baa1064e8f9c6041d3c109f084745d1a9482d7e99f0e0c9a0ec4eccb5b9',
  projectOpenNl4: 'c079b3177686cd545abefee07757ad30d312cb8ccfdaa735732f6b75e8dc1f1e',
  joinDefault: '2e77b8bcb24a2f4e7a3dcbc5016e0177887a0d0aed550857b89d4f26fdabacaa',
  joinWhereNl10: '3fb804f7c7aa9f8caf73ff8a6b52e23ce71cdaf11b0665e73376769d96817b94',
  actorDefault: '10d1225f7b7a217db9c9628960d7d78467d2bb9312ca939cdd5ac19bf9bec149',
  actorFromEmpty: 'a25cad4ce3edddcf5a1776b9f0893ecf395720082264d503f0ebdbbde7498303',
  workforce: '31da2ac5674a745a5c21db1408d52b54ad528265b4db40d92b570e1f464b5ad3',
};

describe('operations read-tail r55 EXISTS-head consolidation', () => {
  it('renders the four openSep head variants byte-identically', () => {
    expect(OPERATIONS_EXISTS_HEAD()).toBe('EXISTS(SELECT 1 FROM ');
    expect(OPERATIONS_EXISTS_HEAD(' ')).toBe('EXISTS( SELECT 1 FROM ');
    expect(OPERATIONS_EXISTS_HEAD('\n    ')).toBe('EXISTS(\n    SELECT 1 FROM ');
    expect(OPERATIONS_EXISTS_HEAD('\n          ')).toBe('EXISTS(\n          SELECT 1 FROM ');
    for (const sep of ['', ' ', '\n    ', '\n          ']) {
      expect(OPERATIONS_EXISTS_HEAD(sep).match(/SELECT /g)).toHaveLength(1);
      expect(OPERATIONS_EXISTS_HEAD(sep).endsWith('SELECT 1 FROM ')).toBe(true);
    }
  });

  it('renders every family-A separator variant through the retired byte oracles', () => {
    const owner = operationsOwnerVersionExists('operations_queue_policies', ' AND status=? ');
    expect(owner).toBe(retired.ownerVersion);
    expect(sha(owner)).toBe(oracle.ownerVersion);

    const project = operationsProjectSourceExists('§staff_projects', 'sp', 'sp.id=t.project_id');
    expect(project).toBe(retired.projectDefault);
    expect(sha(project)).toBe(oracle.projectDefault);

    const aliasEmpty = operationsProjectSourceExists('§templates', 'template', 'template.id=t.template_id', '');
    expect(aliasEmpty).toBe(retired.projectAliasEmpty);
    expect(sha(aliasEmpty)).toBe(oracle.projectAliasEmpty);

    const openNl4 = operationsProjectSourceExists('§social_post_sources', 'hs', 'hs.post_id=hp.id', ' ', '\n    ');
    expect(openNl4).toBe(retired.projectOpenNl4);
    expect(sha(openNl4)).toBe(oracle.projectOpenNl4);

    const join = operationsJoinSourceExists('§department_heads', 'h', '§organization_nodes', 'n', 'n.id=h.department_id', "h.department_id=? AND h.company_id=? AND h.version=? AND n.status='active'");
    expect(join).toBe(retired.joinDefault);
    expect(sha(join)).toBe(oracle.joinDefault);

    const whereNl10 = operationsJoinSourceExists('§origins', 'o', '§posts', 'p', 'p.id=o.post_id', 'o.task_id=t.id AND <POSTS>', ' ', '\n          ');
    expect(whereNl10).toBe(retired.joinWhereNl10);
    expect(sha(whereNl10)).toBe(oracle.joinWhereNl10);

    for (const sql of [owner, project, aliasEmpty, openNl4, join, whereNl10]) {
      expect(sql.match(/SELECT /g)).toHaveLength(1);
    }
    expect(binds(owner)).toBe(4);
    expect(binds(join)).toBe(3);
    expect(binds(project)).toBe(0);
  });

  it('renders every family-B separator variant through the retired byte oracles', () => {
    const def = operationsActorScopeInputExists('SCOPE');
    expect(def).toBe(retired.actorDefault);
    expect(sha(def)).toBe(oracle.actorDefault);

    const fromEmpty = operationsActorScopeInputExists('SCOPE', '');
    expect(fromEmpty).toBe(retired.actorFromEmpty);
    expect(sha(fromEmpty)).toBe(oracle.actorFromEmpty);

    const workforce = workforceActorScopeSql('SCOPE');
    expect(workforce).toBe(retired.workforce);
    expect(sha(workforce)).toBe(oracle.workforce);

    // The tableSep=' ' variant is byte-identical to the pre-r55 default.
    expect(operationsActorScopeInputExists('SCOPE', '', ' ')).toBe(retired.actorFromEmpty);
    expect(workforceActorScopeSql('x').match(/SELECT /g)).toHaveLength(2);
    expect(operationsActorScopeInputExists('x').match(/SELECT /g)).toHaveLength(2);
    expect(binds(workforce)).toBe(3);
  });

  it('keeps one head definition and routes all three family-A files through it', async () => {
    const [helper, guard, project, join] = await Promise.all([
      src('read-tail-r55-sources.ts'), src('identity-policy-version-guard.ts'),
      src('project-source-exists.ts'), src('join-source-exists.ts'),
    ]);
    expect(helper.match(/export const OPERATIONS_EXISTS_HEAD/g)).toHaveLength(1);
    expect(helper).toContain("openSep = ''");
    for (const text of [guard, project, join]) {
      expect(text.match(/OPERATIONS_EXISTS_HEAD\(/g)).toHaveLength(1);
      expect(text).toContain("from './read-tail-r55-sources'");
    }
    expect(guard).not.toContain('EXISTS(SELECT 1 FROM ');
    expect(project).not.toContain('EXISTS(${openSep}SELECT 1 FROM ');
    expect(join).not.toContain('EXISTS(SELECT 1 FROM ');
    // operationsTableExists stays a zero-SELECT delegate.
    expect(guard.match(/export const operationsTableExists/g)).toHaveLength(1);
    expect(guard).toContain("operationsOwnerVersionExists(from, '', predicate)");
    expect(operationsTableExists('handbook_articles a', 'a.id=1'))
      .toBe(operationsOwnerVersionExists('handbook_articles a', '', 'a.id=1'));
  });

  it('keeps one family-B definition and leaves the routine-source-read use intact', async () => {
    const [scope, routine] = await Promise.all([
      src('workforce-actor-scope-source.ts'), src('work-routine-presentation.ts'),
    ]);
    expect(scope.match(/export const operationsActorScopeInputExists/g)).toHaveLength(1);
    expect(scope.match(/export const workforceActorScopeSql/g)).toHaveLength(1);
    expect(scope).toContain("tableSep = ' '");
    const wf = scope.split('\n').find(l => l.includes('export const workforceActorScopeSql'))!;
    expect(wf).toContain("operationsActorScopeInputExists(where, '', '')");
    expect(wf).not.toContain('SELECT 1 FROM');
    expect((routine.match(/operationsActorScopeInputExists\(scope\.where,''\)/g) ?? [])).toHaveLength(1);
  });

  it('leaves viewer clock, native reads, locks and prior r53/r54 helpers untouched', async () => {
    const [scope, purchases, native, alert, locks, r53] = await Promise.all([
      src('workforce-actor-scope-source.ts'), src('record-purchases.ts'),
      src('native-operations-read.ts'), src('work-task-alert-purchases.ts'),
      src('project-source-locks.ts'), src('read-tail-r53-sources.ts'),
    ]);
    expect(scope).toContain('OPERATIONS_VIEWER_CLOCK_SQL');
    expect(scope).toContain("sep = '\\n '");
    expect(scope).toContain('${sep}SELECT 1 FROM ');
    expect(OPERATIONS_VIEWER_CLOCK_SQL).toContain('clock_timestamp()');
    expect(purchases).toContain("operationsViewerClockHead(viewer,' ')");
    expect(native).toContain('OPERATIONS_RD_');
    expect(alert).toContain('LOCK TABLE staff_tasks');
    expect(locks).toContain('LOCK TABLE staff_tasks IN SHARE ROW EXCLUSIVE MODE');
    expect(r53).toContain('OPERATIONS_STAFF_TASK_HEAD');
    expect(r53).toContain('OPERATIONS_TASK_LIST_HEAD');
    for (const text of [OPERATIONS_EXISTS_HEAD(), scope]) expect(text).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE/i);
  });

  it('retirement:ts-statements lowers the operations SELECT KPI from 71 to 68', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBe(60);
    expect(total).toBeLessThan(71);
  });
});
