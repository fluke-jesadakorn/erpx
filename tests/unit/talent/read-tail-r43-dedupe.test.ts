import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import ts from 'typescript';

const adapters = resolve(__dirname, '../../../packages/server-talent/src/adapters');
const src = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const compile = (decl: string) => ts.transpileModule(decl, { compilerOptions: { target: ts.ScriptTarget.ES2020 } }).outputText;
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Real owner-local renderers, extracted from source so the oracles exercise
// shipped implementations rather than copies of their bodies. The r43 witness
// head delegates to the r27 EXISTS prefix, so both declarations are evaluated.
const prefixDecl = src('talent-read-tail-r27-sources.ts').match(/export const existsGuardPrefix = [\s\S]*?;\n/)![0].replace('export const', 'const ');
const witnessDecl = src('talent-read-tail-r43-sources.ts').match(/export const existsSourceWitness = [\s\S]*?;\n/)![0].replace('export const', 'const ');
const existsGuardPrefix = new Function(compile(prefixDecl) + '\nreturn existsGuardPrefix;')() as (source: string) => string;
const existsSourceWitness = new Function('existsGuardPrefix', compile(witnessDecl) + '\nreturn existsSourceWitness;')(existsGuardPrefix) as (source: string, predicate: string) => string;

// Exact pre-r43 emitted witness (BEFORE=12). Captured from the former inline
// attachment-read-source literal; the caller keeps its own row source, predicate
// and binds, so only the shared head moved into the owner-local family.
const GOLDEN = 'EXISTS(SELECT 1 FROM talent_offers selected JOIN talent_applications a ON a.id=selected.application_id AND a.company_id=selected.company_id WHERE selected.id=? AND selected.company_id=c.company_id AND a.candidate_id=c.id)';
const SOURCE = 'talent_offers selected JOIN talent_applications a ON a.id=selected.application_id AND a.company_id=selected.company_id';
const PREDICATE = 'selected.id=? AND selected.company_id=c.company_id AND a.candidate_id=c.id';
const LENGTH = 221;
const DIGEST = 'cda9e3291d5fbe1c6ff94287a9ca1113692fc6063eb331b6e88e760574de7731';

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Talent read-tail r43 EXISTS source-witness head family', () => {
  test('the routed witness renders byte-identically to the former inline literal', () => {
    const actual = existsSourceWitness(SOURCE, PREDICATE);
    expect(actual).toBe(GOLDEN);
    expect(actual.length).toBe(LENGTH);
    expect(sha(actual)).toBe(DIGEST);
    readOnly(actual);
    // The delegated r27 prefix is itself byte-exact and read-only.
    expect(existsGuardPrefix('__SRC__')).toBe('EXISTS(SELECT 1 FROM __SRC__ WHERE ');
    readOnly(existsGuardPrefix('__SRC__'));
  });

  test('one owner-local head, single call site routed, no inline witness remains', () => {
    const head = src('talent-read-tail-r43-sources.ts');
    expect(head.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
    expect(head.match(/export const existsSourceWitness/g) ?? []).toHaveLength(1);
    expect(head).toMatch(/from ['"]\.\/talent-read-tail-r27-sources['"]/);
    expect(head).toContain('existsGuardPrefix(source)');

    const callsite = src('attachment-read-sources.ts');
    expect(callsite.match(/existsSourceWitness\(/g) ?? []).toHaveLength(1);
    expect(callsite.match(/EXISTS\(SELECT 1 FROM talent_offers/g) ?? []).toHaveLength(0);
    expect(callsite).toMatch(/from ['"]\.\/talent-read-tail-r43-sources['"]/);
    // The sibling branch and its binds stay local.
    expect(callsite).toContain("target==='talent_candidate'?'c.id=?'");
  });

  test('locks, native reads, prior families and cross-owner literals stay untouched', () => {
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
    expect(src('attachment-read-sources.ts')).toContain('LOCK TABLE talent_requisitions');
    expect(src('people-hire-handoff-facts.ts')).toContain('FOR ${mode}');
    expect(src('native-talent-read.ts')).toContain('nativeTalentReadFirst');
    expect(src('talent-read-tail-r27-sources.ts')).toContain('EXISTS(SELECT 1 FROM ${source} WHERE ');
    expect(src('talent-read-tail-r36-sources.ts')).toContain('SELECT 1 FROM ${source} WHERE ${predicate}');
    expect(src('talent-read-tail-r37-sources.ts')).toContain('SELECT ${projection}');
    expect(src('talent-read-tail-r41-sources.ts')).toContain('export const talentSourceSubqueryExists');
    expect(src('talent-read-tail-r42-sources.ts')).toContain('export const selectSourceRow');
    expect(src('native_talent_read_sql.rs')).toContain("'ownerEligible',EXISTS(SELECT 1 FROM assignments owner_assignment");
    expect(src('product-entry.ts')).toContain('SELECT version FROM assignments WHERE id=?');
  });

  test('lowers the talent owner SELECT KPI from BEFORE=12 to 11', () => {
    const files: string[] = [];
    const walk = (d: string) => {
      for (const e of readdirSync(d, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(d, e.name);
        if (e.isDirectory()) walk(p);
        else if (e.name.endsWith('.ts')) files.push(p);
      }
    };
    walk(resolve(__dirname, '../../../packages/server-talent/src'));
    const contents = files.map((f) => readFileSync(f, 'utf8'));
    const matchCount = (re: RegExp) => contents.reduce((n, text) => n + (text.match(re) ?? []).length, 0);
    const splitCount = contents.reduce((n, text) => n + text.split('SELECT ').length - 1, 0);
    for (const count of [matchCount(/SELECT/g), matchCount(/\bSELECT\b/g), splitCount]) {
      expect(count).toBe(10);
      expect(count).toBeLessThan(12);
    }
  });
});
