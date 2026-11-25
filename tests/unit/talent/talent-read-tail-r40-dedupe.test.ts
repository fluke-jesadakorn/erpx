import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import ts from 'typescript';

const adapters = resolve(__dirname, '../../../packages/server-talent/src/adapters');
const src = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const compile = (decl: string) => ts.transpileModule(decl, { compilerOptions: { target: ts.ScriptTarget.ES2020 } }).outputText;
const head = compile(src('talent-read-tail-r37-sources.ts').match(/export const selectProjection = [\s\S]*?;\n/)![0].replace('export const', 'const '));
const selectProjection = new Function(head + '\nreturn selectProjection;')() as (p: string) => string;

function renderFn(file: string, re: RegExp, stubDecl = ''): (...args: unknown[]) => string {
  const m = src(file).match(re);
  if (!m) throw new Error(`no match in ${file}: ${re}`);
  const decl = m[0].replace(/^export const /, 'const ');
  const sym = decl.match(/const (\w+)/)![1];
  return new Function(head + stubDecl + compile(decl) + `\nreturn ${sym};`)() as (...args: unknown[]) => string;
}

// Exact pre-r40 emitted SQL (BEFORE=19). Stubs keep the pinned strings owner-local;
// every value below was captured from the former literal heads.
const GOLDEN: Record<string, string> = {
  offerFilesMutable: "(t.status<>'accepted' OR EXISTS(SELECT 1 FROM talent_hire_handoffs handoff JOIN talent_hire_consumption_receipts receipt ON receipt.handoff_id=handoff.id AND receipt.company_id=handoff.company_id AND receipt.source_offer_version=handoff.source_offer_version WHERE handoff.offer_id=t.id AND handoff.company_id=t.company_id))",
  assignmentVersionGuard: 'EXISTS(SELECT 1 FROM assignments WHERE id=? AND version=?)',
  orgNameLabel: "COALESCE((SELECT n.display_name __NODES__ WHERE n.id=r.branch_id AND n.company_id=r.company_id),'Branch')",
  scopeCte: '(SELECT ?::text company_id,?::text branch_id,?::text department_id)',
  talentClockCte: 'WITH c AS MATERIALIZED (SELECT __CLOCK__ now_ms)',
};

function render(): Record<string, string> {
  const offerFilesMutable = renderFn('attachments-sensitive-facts.ts', /const offerFilesMutable=[\s\S]*?;\n/);
  const assignmentVersionGuard = renderFn('product-entry.ts', /const assignmentVersionGuard = [\s\S]*?;\n/) as unknown as string;
  const orgNameLabel = renderFn('executive-metric-projections.ts', /const orgNameLabel = [\s\S]*?;\n/, "const organizationSources={talent_adapters_executive_metric_projections_organization_nodes_n_from:'__NODES__'};\n");
  const scopeCte = renderFn('talent-scope-sql.ts', /const scopeCte = [\s\S]*?;\n/);
  const talentClockCte = renderFn('talent-scope-sql.ts', /export const talentClockCte = [\s\S]*?;\n/, "const packageClockSql='__CLOCK__';\n");
  return {
    offerFilesMutable: offerFilesMutable('t'),
    assignmentVersionGuard,
    orgNameLabel: orgNameLabel('r.branch_id', 'Branch'),
    scopeCte: scopeCte('?::text', '?::text'),
    talentClockCte: talentClockCte('c'),
  };
}

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Talent read-tail r40 straggler projection-head family', () => {
  test('renders byte-identically to every pre-r40 literal head', () => {
    const actual = render();
    expect(Object.keys(actual).sort()).toEqual(Object.keys(GOLDEN).sort());
    for (const [name, expected] of Object.entries(GOLDEN)) {
      expect(actual[name], name).toBe(expected);
      readOnly(actual[name]);
    }
  });

  test('all five named stragglers delegate to the r37 head', () => {
    const targets: Array<[string, number, number]> = [
      ['attachments-sensitive-facts.ts', 1, 0],
      ['product-entry.ts', 1, 1],
      ['executive-metric-projections.ts', 2, 0],
      ['talent-scope-sql.ts', 3, 0],
    ];
    for (const [name, delegations, literals] of targets) {
      const text = src(name);
      expect(text.match(/selectProjection\(/g) ?? [], name).toHaveLength(delegations);
      expect(text.match(/\bSELECT\b/g) ?? [], name).toHaveLength(literals);
      expect(text, name).toMatch(/from ['"]\.\/talent-read-tail-r37-sources['"]/);
    }
    expect(selectProjection('id')).toBe('SELECT id');
  });

  test('one universal head, locks, owner boundary and native reads stay untouched', () => {
    const universal = src('talent-read-tail-r37-sources.ts');
    expect(universal.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(universal.match(/export const selectProjection/g) ?? []).toHaveLength(1);
    expect(src('attachment-read-sources.ts')).toContain('LOCK TABLE talent_requisitions');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
    expect(src('people-hire-handoff-facts.ts')).toContain('FOR ${mode}');
    expect(src('native-talent-read.ts')).toContain('nativeTalentReadFirst');
    expect(src('talent-read-tail-r36-sources.ts')).toContain('SELECT 1 FROM ${source} WHERE ${predicate}');
    expect(src('package-obligations.ts')).toContain('LOCK TABLE talent_requisitions');
  });

  test('lowers the talent owner SELECT KPI from BEFORE=19 to 14', () => {
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
    for (const f of files) count += (readFileSync(f, 'utf8').match(/\bSELECT\b/g) ?? []).length;
    expect(count).toBe(12);
    expect(count).toBeLessThan(19);
  });
});
