import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import ts from 'typescript';

const adapters = resolve(__dirname, '../../../packages/server-talent/src/adapters');
const src = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const compile = (decl: string) => ts.transpileModule(decl, { compilerOptions: { target: ts.ScriptTarget.ES2020 } }).outputText;
const sha = (s: string) => createHash('sha256').update(s).digest('hex').slice(0, 16);

// Real owner-local renderer, extracted from source so the oracles exercise the
// shipped implementation rather than a copy of its body.
const headDecl = src('talent-read-tail-r41-sources.ts').match(/export const talentSourceSubqueryExists = [\s\S]*?;\n/)![0].replace('export const', 'const ');
const talentSourceSubqueryExists = new Function(compile(headDecl) + '\nreturn talentSourceSubqueryExists;')() as (source: string, body: string) => string;

// Exact pre-r41 rendered fragments (BEFORE=14). Captured from the former
// inline `SELECT 1 FROM (${sources}) ` literals; stubs keep every value local,
// the shared head is the only thing that moved.
const GOLDEN = {
  r22: "EXISTS(__CLOCK__(talent_source_clock) SELECT 1 FROM (__SOURCES__) talent_source CROSS JOIN __SCOPE__ talent_viewer CROSS JOIN talent_source_clock WHERE talent_source.id=__ID__ AND talent_source.company_id=__COMPANY__ AND talent_source.company_id=talent_viewer.company_id AND TRUE  AND (talent_source.kind='candidate' OR (talent_viewer.branch_id IS NULL OR talent_source.branch_id=talent_viewer.branch_id) AND (talent_viewer.department_id IS NULL OR talent_source.department_id=talent_viewer.department_id)) AND __READ__(talent_viewer,records.read) AND __READ__(talent_viewer,records.review) AND (__PURCHASE__))",
  r29: "(NOT(module_id='talent' AND EXISTS(SELECT 1 FROM (__SOURCES__) actual_talent_source WHERE actual_talent_source.id=__ID__)) OR __SCOPEWHERE__)",
};
const DIGEST = { r22: ['e5643328d7a3c858', 610], r29: ['fa3a3cfed04abc1f', 141] } as const;

function template(afterMarker: string, endMarker: string): string {
  const text = src('record-purchases.ts');
  const start = text.indexOf(afterMarker);
  if (start < 0) throw new Error(`no marker ${afterMarker}`);
  const tick = text.indexOf('`', start);
  const end = text.indexOf(endMarker, tick + 1);
  if (end < 0) throw new Error(`no end marker ${endMarker}`);
  return text.slice(tick, end);
}

function render(tpl: string): string {
  const stubs = {
    talentClockCte: (name: string) => `__CLOCK__(${name})`,
    sources: '__SOURCES__',
    talentActorScopeCte: '__SCOPE__',
    assignmentReadSql: (a: string, p: string) => `__READ__(${a},${p})`,
    purchasedCapabilityPredicate: (a: string, b: string, c: string, d: string) => `__PURCH__(${a},${b},${c},${d})`,
    permission: true,
    kind: undefined,
    id: '__ID__',
    company: '__COMPANY__',
    capabilities: ['a', 'b', 'c', 'd'],
    purchase: '__PURCHASE__',
    talentSourceSubqueryExists,
    scope: { where: '__SCOPEWHERE__' },
  };
  const names = Object.keys(stubs);
  return new Function(...names, `return ${tpl};`)(...names.map((n) => (stubs as Record<string, unknown>)[n])) as string;
}

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Talent read-tail r41 subquery-source existence head family', () => {
  test('both guard sites render byte-identically to the pre-r41 inline literals', () => {
    const actual = {
      r22: render(template('return {where:', ',args:[actor.companyId')),
      r29: render(template('if(surface==="records")return {where:', ',args:scope.args')),
    };
    for (const key of ['r22', 'r29'] as const) {
      expect(actual[key], key).toBe(GOLDEN[key]);
      expect(actual[key].length, key).toBe(DIGEST[key][1]);
      expect(sha(actual[key]), key).toBe(DIGEST[key][0]);
      readOnly(actual[key]);
    }
  });

  test('one owner-local head, both call sites routed, no literal remains', () => {
    const head = src('talent-read-tail-r41-sources.ts');
    expect(head.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(head.match(/export const talentSourceSubqueryExists/g) ?? []).toHaveLength(1);
    expect(talentSourceSubqueryExists('__SRC__', '__BODY__')).toBe('SELECT 1 FROM (__SRC__) __BODY__');
    readOnly(talentSourceSubqueryExists('__SRC__', '__BODY__'));

    const callsite = src('record-purchases.ts');
    expect(callsite.match(/talentSourceSubqueryExists\(/g) ?? []).toHaveLength(2);
    expect(callsite.match(/SELECT 1 FROM \(/g) ?? []).toHaveLength(0);
    expect(callsite).toMatch(/from ['"]\.\/talent-read-tail-r41-sources['"]/);
  });

  test('locks, native reads, prior families and cross-owner literals stay untouched', () => {
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
    expect(src('attachment-read-sources.ts')).toContain('LOCK TABLE talent_requisitions');
    expect(src('people-hire-handoff-facts.ts')).toContain('FOR ${mode}');
    expect(src('native-talent-read.ts')).toContain('nativeTalentReadFirst');
    expect(src('talent-read-tail-r27-sources.ts')).toContain('EXISTS(SELECT 1 FROM ${source} WHERE ');
    expect(src('talent-read-tail-r36-sources.ts')).toContain('SELECT 1 FROM ${source} WHERE ${predicate}');
    expect(src('talent-read-tail-r37-sources.ts')).toContain('SELECT ${projection}');
    expect(src('purchases.ts')).toContain('EXISTS(SELECT 1 FROM assignments owner_assignment');
  });

  test('lowers the talent owner SELECT KPI from BEFORE=14 to 13', () => {
    const files: string[] = [];
    const walk = (d: string) => {
      for (const e of readdirSync(d, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(d, e.name);
        if (e.isDirectory()) walk(p);
        else if (e.name.endsWith('.ts')) files.push(p);
      }
    };
    walk(resolve(__dirname, '../../../packages/server-talent/src'));
    for (const re of [/SELECT/g, /\bSELECT\b/g]) {
      let count = 0;
      for (const f of files) count += (readFileSync(f, 'utf8').match(re) ?? []).length;
      expect(count).toBe(12);
      expect(count).toBeLessThan(14);
    }
  });
});
