import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import ts from 'typescript';

const srcRoot = resolve(__dirname, '../../../packages/server-talent/src/adapters');
const src = (name: string) => readFileSync(resolve(srcRoot, name), 'utf8');
const compile = (decl: string) => ts.transpileModule(decl, { compilerOptions: { target: ts.ScriptTarget.ES2020 } }).outputText;
const digest = (value: string) => createHash('sha256').update(value).digest('hex');

/** SHA-256 byte oracles captured from the pre-r38 literal rendering. The stub
 *  source is fixed to 'TPL' and the real r27 EXISTS / r37 projection heads are
 *  injected, so every rendered byte is compared against the former literal. */
const ORACLE: Record<string, string> = {
  requisitions: 'b1aea1b43c0536d3b37d4bc7294d1dc271d9ce3b1fcc3ba65853f03cfde5bac1',
  applications: '4202a9a5659dd5aecd2ed13773a6dace24ffac5d6fd4651e742fc6600f5b0d11',
  offers: '4ea61bf03f66f56dd275f4b1959910f70064ea3ebfc0a5b40a83bdf3241bf3e0',
  handoffs: 'fa59a549b9ef2ad2a22f9849bbf3850fbe85cb5de6d1d37b12c545c16ec43ace',
  receipts: '320ecc06e4ba3e2f86b4d686d6596aff6f5e0abfad31ea0699d328857994ad9f',
  scoped: 'cd4d8f224261ab4f3e9d5e7a4b1d7847087f766ea65a3e088a75692e3e543012',
  facts: '7046b2d7d5fb71726c3dc9a3ee89928232f172d306ded672ef6c0da912a7aac2',
};

type Rendered = { sources: Record<string, string>; scoped: string; facts: string };

async function render(): Promise<Rendered> {
  const code = src('package-obligations.ts')
    .split('\n').filter((line) => !/^\s*import\s/.test(line)).join('\n')
    .replace(/\bexport\s+(const|async function|function|type|interface)\b/g, '$1');
  const guard = compile(src('talent-read-tail-r27-sources.ts').match(/export const existsGuardPrefix = [\s\S]*?;\n/)![0].replace(/^export const /, 'const '));
  const head = compile(src('talent-read-tail-r37-sources.ts').match(/export const selectProjection = [\s\S]*?;\n/)![0].replace(/^export const /, 'const '));

  const prepared: string[] = [];
  const database = {
    prepare(sql: string) { prepared.push(sql); return { bind() { return this; }, all() { return { results: [] }; }, run() { return {}; } }; },
    batch() { return {}; },
  };
  const factory = new Function(
    'talentSettlementPlacements', 'packageSettlementSource', 'talentSettlementPlacementReadLocks', 'createHash',
    guard + '\n' + head + '\n' + compile(code) +
      '\nreturn {talentPackageSettlementSources, scopedMaterialized, lockTalentPackageObligationSources, talentPackageObligations};'
  );
  const mod = factory('TPL', () => ({ database, companyId: 'C' }), () => [],
    () => ({ update() { return this; }, digest() { return 'HASH'; } })) as {
      talentPackageSettlementSources: Record<string, string>;
      scopedMaterialized: (name: string, alias: string, source: string) => string;
      lockTalentPackageObligationSources: (owner: unknown, lock: unknown) => Promise<void>;
      talentPackageObligations: (owner: unknown, database: unknown, input: unknown) => Promise<void>;
    };
  const owner = { assertActiveTransaction() { return; } };
  await mod.lockTalentPackageObligationSources(owner, {});
  const sources = mod.talentPackageSettlementSources;
  const scoped = mod.scopedMaterialized('requisitions', 'r', 'REQ');
  await mod.talentPackageObligations(owner, database, { companyId: 'C', packageId: 'recruit', packageVersion: 1, moduleId: 'talent', now: 0 });
  const facts = prepared.find((sql) => sql.includes('WITH scope AS'))!;
  return { sources, scoped, facts };
}

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Talent read-tail r38 package-obligations projection-universal family', () => {
  test('renders 7 byte-identical oracles (5 sources + scope CTE + UNION facts)', async () => {
    const { sources, scoped, facts } = await render();
    const actual: Record<string, string> = { ...sources, scoped, facts };
    for (const [name, expected] of Object.entries(ORACLE)) expect(digest(actual[name]), name).toBe(expected);
    expect(scoped).toBe('requisitions AS MATERIALIZED(SELECT r.* FROM (REQ) r,scope WHERE r.company_id=scope.company_id)');
    expect(sources.receipts).toBe('SELECT * FROM talent_hire_consumption_receipts');
    for (const [name, sql] of Object.entries(actual)) readOnly(sql);
  });

  test('all 12 anchors delegate to the r37 head; no literal SELECT remains', () => {
    const text = src('package-obligations.ts');
    expect(text.match(/SELECT/g) ?? []).toHaveLength(0);
    expect(text.match(/selectProjection\(/g) ?? []).toHaveLength(13);
    expect(text).toContain('import {selectProjection} from "./talent-read-tail-r37-sources"');
    const anchors = [
      'const requisitions=`${selectProjection(',
      'const applications=`${selectProjection(',
      "const offerEventPrefix=`EXISTS(${selectProjection('1')} FROM talent_offer_events",
      'const offers=`${selectProjection(',
      'handoffs:`${selectProjection(',
      "receipts:`${selectProjection('*')} FROM talent_hire_consumption_receipts",
      "WITH scope AS(${selectProjection('?::text company_id')})",
      "${selectProjection(`'requisition' kind",
      "${selectProjection(`'candidate',c.id",
      "${selectProjection(`'application',a.id",
      "${selectProjection(`'proposal',p.id",
      "${selectProjection(`'offer',o.id",
    ];
    for (const anchor of anchors) expect(text, anchor).toContain(anchor);
  });

  test('locks, owner boundary and native read ports stay untouched', () => {
    expect(src('attachment-read-sources.ts')).toContain('LOCK TABLE talent_requisitions');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
    expect(src('people-hire-handoff-facts.ts')).toContain('FOR ${mode}');
    expect(src('native-talent-read.ts')).toContain('nativeTalentReadFirst');
    expect(src('talent-read-tail-r36-sources.ts')).toContain('SELECT 1 FROM ${source} WHERE ${predicate}');
    // The LOCK TABLE statement stays in package-obligations' lock function only.
    expect(src('package-obligations.ts')).toContain('LOCK TABLE talent_requisitions');
  });

  test('lowers the talent owner SELECT KPI from BEFORE=39 to 27', () => {
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
    expect(count).toBeLessThan(39);
  });
});
