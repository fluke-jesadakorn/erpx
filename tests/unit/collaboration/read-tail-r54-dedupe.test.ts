import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { workScopeSourceCte } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const shared = src('read-tail-sql.ts');
const obligations = src('work-package-obligations.ts');
const rendererLine = shared.split('\n').find(l => l.startsWith('export const workScopeSourceCte='))!;

// Exact pre-r54 source lines frozen at the live HEAD before the fold. The two
// Work source CTEs were byte-identical apart from the CTE name and the
// registered rowset; the scope cross-join, projection, zero binds and lock
// statements at the call site never varied.
const TASKS = {
  name: 'tasks',
  rows: '${operations.tasks}',
  line: ' tasks AS MATERIALIZED(SELECT t.* FROM (${operations.tasks})t CROSS JOIN scope WHERE t.company_id=scope.company_id AND t.project_id IS NULL),',
  lineSha: '65c243a676ccca4a1663be9d1f840402944caa20f7583ce68d2c60fb88d7a9bb',
};
const TEMPLATES = {
  name: 'templates',
  rows: '${operations.templates}',
  line: ' templates AS MATERIALIZED(SELECT t.* FROM (${operations.templates})t CROSS JOIN scope WHERE t.company_id=scope.company_id AND t.project_id IS NULL),',
  lineSha: '33d88c53bdd520e164f9bf92b63bfd93001b7842ffc20f2105c445546e241c1f',
};
const lockRe = /INSERT\s+INTO|UPDATE\s+\w+\s+SET|DELETE\s+FROM|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;

describe('Collaboration read-tail r54 Work scope-source CTE family', () => {
  for (const site of [TASKS, TEMPLATES]) {
    test(`${site.name}: the pre-fold line renders byte for byte`, () => {
      expect(sha(site.line)).toBe(site.lineSha);
      expect(site.line.match(/SELECT /g) ?? []).toHaveLength(1);
      expect(site.line).not.toMatch(/\?/);
      const rendered = ` ${workScopeSourceCte(site.name, site.rows)}`;
      expect(rendered).toBe(site.line);
      expect(sha(rendered)).toBe(site.lineSha);
    });
  }

  test('the two rowsets and CTE names are distinct; only the shared body folds', () => {
    expect(TASKS.rows).not.toBe(TEMPLATES.rows);
    expect(TASKS.name).not.toBe(TEMPLATES.name);
    expect(workScopeSourceCte(TASKS.name, TASKS.rows)).not.toBe(workScopeSourceCte(TEMPLATES.name, TEMPLATES.rows));
    for (const site of [TASKS, TEMPLATES])
      expect(workScopeSourceCte(site.name, site.rows)).toContain('CROSS JOIN scope WHERE t.company_id=scope.company_id AND t.project_id IS NULL');
  });

  test('one renderer owns the shared body and both call sites route through it', () => {
    expect(shared.match(/export const workScopeSourceCte=/g) ?? []).toHaveLength(1);
    expect(sha(rendererLine)).toBe(sha('export const workScopeSourceCte=(name:string,rows:string)=>`${name} AS MATERIALIZED(SELECT t.* FROM (${rows})t CROSS JOIN scope WHERE t.company_id=scope.company_id AND t.project_id IS NULL),`;'));
    expect(rendererLine.match(/SELECT /g) ?? []).toHaveLength(1);
    expect(obligations).toContain("${workScopeSourceCte('tasks',operations.tasks)}");
    expect(obligations).toContain("${workScopeSourceCte('templates',operations.templates)}");
    expect(obligations.split('workScopeSourceCte').length - 1).toBe(3); // import + 2 call sites
    expect(obligations).not.toContain('tasks AS MATERIALIZED(SELECT t.* FROM (${operations.tasks})');
    expect(obligations).not.toContain('templates AS MATERIALIZED(SELECT t.* FROM (${operations.templates})');
  });

  test('stays read-only and leaves the caller scope, binds and locks untouched', () => {
    expect(rendererLine).not.toMatch(lockRe);
    expect(obligations).toContain('WITH scope AS(SELECT ?::text company_id,?::text package_id,?::bigint now_ms');
    expect(obligations).toContain('.bind(input.companyId,input.packageId,input.now,input.now)');
    expect(obligations).toContain('LOCK TABLE social_posts,social_post_sources');
  });
});

describe('Collaboration read-tail r54 retirement check', () => {
  test('retirement:ts-statements drops 320 -> 319', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // Two byte-identical source CTE heads -> one parameterized renderer.
    // -2 at the call site, +1 in the shared helper, net -1.
    expect(total).toBe(308);
    expect(total).toBeLessThan(320);
  });
});
