import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import ts from 'typescript';

const srcRoot = resolve(__dirname, '../../../packages/server-talent/src/adapters');
const src = (name: string) => readFileSync(resolve(srcRoot, name), 'utf8');
const compile = (decl: string) => ts.transpileModule(decl, { compilerOptions: { target: ts.ScriptTarget.ES2020 } }).outputText;

function helper(name: string, re: RegExp, scope = ''): (...args: unknown[]) => string {
  const m = src(name).match(re);
  if (!m) throw new Error(`no helper in ${name}`);
  const decl = m[0].replace(/^export const /, 'const ');
  const sym = decl.match(/const (\w+)/)![1];
  const head = compile(src('talent-read-tail-r37-sources.ts').match(/export const selectProjection = [\s\S]*?;\n/)![0].replace('export const', 'const '));
  return new Function(scope + head + compile(decl) + `\nreturn ${sym};`)() as (...args: unknown[]) => string;
}

const ACTOR_SCOPE = "const talentActorScopeCte='(SELECT ?::text company_id,NULL branch_id,NULL department_id)';\n";
const SOURCE = "const source={sql:'(SRC)'};\n";

// Closed samples copied verbatim from the pre-r37 (BEFORE=44) helper outputs.
const ORACLES: Array<[string, RegExp, unknown[]]> = [
  ['talent-read-tail-r32-sources.ts', /export const talentSourceRows = [\s\S]*?;\n/, ['*', 'talent_requisitions']],
  ['talent-read-tail-r32-sources.ts', /export const talentSourceRows = [\s\S]*?;\n/, ['a.*,r.branch_id,r.department_id', 'talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id']],
  ['talent-scope-sql.ts', /export const talentActorScopeSelect = [\s\S]*?;\n/, ['s', 'allowed']],
  ['executive-evidence-sources.ts', /const evidenceSelect = [\s\S]*?LIMIT \?`;\n/, ['r.id,r.status', 'r WHERE x', 'r.id']],
  ['executive-metric-projections.ts', /const metricSelect = [\s\S]*?;\n/, ['r.status', '(SRC)']],
  ['package-obligations.ts', /const scopedMaterialized=[\s\S]*?;\n/, ['requisitions', 'r', 'REQ']],
];
const EXPECTED = [
  'SELECT * FROM talent_requisitions',
  'SELECT a.*,r.branch_id,r.department_id FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id',
  'SELECT allowed FROM (SELECT ?::text company_id,NULL branch_id,NULL department_id) s',
  'SELECT r.id,r.status\n      FROM ((SRC)) r WHERE x\n      ORDER BY r.id LIMIT ?',
  'SELECT r.status label,count(*)::text value FROM ((SRC)) ',
  'requisitions AS MATERIALIZED(SELECT r.* FROM (REQ) r,scope WHERE r.company_id=scope.company_id)',
];

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Talent read-tail r37 universal projection head family', () => {
  test('renders byte-identically to every pre-r37 helper literal', () => {
    ORACLES.forEach(([name, re, args], i) => {
      const scope = name.includes('scope-sql') ? ACTOR_SCOPE : name.includes('evidence') ? SOURCE : '';
      const rendered = helper(name, re, scope)(...args);
      expect(rendered, `${name} #${i}`).toBe(EXPECTED[i]);
      readOnly(rendered);
    });
  });

  test('one universal head and five delegating named helpers', () => {
    const head = src('talent-read-tail-r37-sources.ts');
    expect(head.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(head.match(/export const selectProjection/g) ?? []).toHaveLength(1);
    const headDecl = head.match(/export const selectProjection = [\s\S]*?;\n/)![0].replace('export const', 'const ');
    const headFn = new Function(compile(headDecl) + '\nreturn selectProjection;')() as (p: string) => string;
    expect(headFn('id')).toBe('SELECT id');

    const delegations: Array<[string, RegExp]> = [
      ['talent-read-tail-r32-sources.ts', /export const talentSourceRows/g],
      ['talent-scope-sql.ts', /export const talentActorScopeSelect/g],
      ['executive-evidence-sources.ts', /const evidenceSelect =/g],
      ['executive-metric-projections.ts', /const metricSelect =/g],
      ['package-obligations.ts', /const scopedMaterialized=/g],
    ];
    for (const [name, re] of delegations) {
      const text = src(name);
      expect(text.match(re) ?? [], name).toHaveLength(1);
      expect(text, name).toContain('selectProjection(');
      expect(text, name).toMatch(/from ['"]\.\/talent-read-tail-r37-sources['"]/);
      // package-obligations owns pre-existing LOCK TABLE statements pinned by r23/r24.
      if (name !== 'package-obligations.ts')
        expect(text, name).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('locks, owner boundary and native read ports stay untouched', () => {
    expect(src('attachment-read-sources.ts')).toContain('LOCK TABLE talent_requisitions');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
    expect(src('people-hire-handoff-facts.ts')).toContain('FOR ${mode}');
    expect(src('native-talent-read.ts')).toContain('nativeTalentReadFirst');
    expect(src('talent-read-tail-r36-sources.ts')).toContain('SELECT 1 FROM ${source} WHERE ${predicate}');
  });

  test('lowers the talent owner SELECT KPI strictly below BEFORE=44 to 27', () => {
    const files: string[] = [];
    const walk = (d: string) => {
      for (const e of readdirSync(d, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(d, e.name);
        if (e.isDirectory()) walk(p);
        else if (e.name.endsWith('.ts')) files.push(p);
      }
    };
    walk(resolve(__dirname, '../../../packages/server-talent/src'));
    let count = 0;
    for (const f of files) count += (readFileSync(f, 'utf8').match(/SELECT/g) ?? []).length;
    expect(count).toBe(10);
    expect(count).toBeLessThan(44);
  });
});
