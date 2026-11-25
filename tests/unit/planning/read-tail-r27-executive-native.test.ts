import { describe, expect, test } from 'vitest';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

const srcRoot = resolve(__dirname, '../../../packages/server-planning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const pub = (name: string) => readFileSync(resolve(srcRoot, 'public', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

describe('Planning r27 executive projection native cutover', () => {
  test('both executive call sites are native-owned and emit no TS SQL', () => {
    const caller = src('production-command-sources-read.ts');
    expect(caller).toContain('nativeExecutiveShortage(recordId,companyId,branchId,from,to)');
    expect(caller).toContain('nativeExecutiveRecommendation(recordId,companyId,branchId,from,to,eligibilityKind)');
    expect(caller.match(/db\.prepare/g) ?? []).toHaveLength(0);
    expect(caller).not.toContain('planningSelectFrom');
    expect(caller).not.toContain('shortageProjection');
    expect(caller).not.toContain('jsonb_path_exists');
  });

  test('the TS transport admits both executive kinds with a closed 28-key row', () => {
    const client = src('native-lane-36/client.ts');
    for (const kind of ["'executive-shortage'", "'executive-recommendation'"])
      expect(client).toContain(`| ${kind}`);
    expect(client).toContain('const EXECUTIVE_KEYS = [');
    expect(client).toContain("if (isExecutiveKind(kind))");
    expect(client).toContain("url.searchParams.set('from', identity.from)");
    expect(client).toContain("url.searchParams.set('to', identity.to)");
    expect(client).toContain("url.searchParams.set('eligibility', identity.eligibility)");
    expect(client).toContain("!inSet(identity.eligibility, ['proposed', 'capacity'])");
    expect(client).toContain('export const nativeExecutiveShortage');
    expect(client).toContain('export const nativeExecutiveRecommendation');
    expect(client.match(/db\.prepare/g) ?? []).toHaveLength(0);
  });

  test('the shared SELECT head is deleted and capacity keeps its single-line builder', () => {
    expect(existsSync(resolve(srcRoot, 'adapters/planning-select-from-sql.ts'))).toBe(false);
    expect(readdirSync(resolve(srcRoot, 'adapters'))).not.toContain('planning-select-from-sql.ts');
    expect(src('capacity.ts')).toContain('planningSelectColumnsFrom(');
    expect(src('capacity.ts')).not.toContain('planningSelectFrom');
  });

  test('the native statements are the byte-exact registered executive plans', () => {
    const postgres = src('native_lane_36/postgres.rs');
    expect(postgres).toContain('LazyLock::new(|| positional(registered_executive::SHORTAGE_EXEC_SQL))');
    expect(postgres).toContain('LazyLock::new(|| positional(registered_executive::PROPOSED_EXEC_SQL))');
    expect(postgres).toContain('LazyLock::new(|| positional(registered_executive::CAPACITY_EXEC_SQL))');
    expect(postgres).toContain('"executive-shortage" | "executive-recommendation" => map_executive(row)');
    const registry = pub('read_source.rs');
    expect(registry).toContain('RD_EXECUTIVE_SHORTAGE\n                    | RD_EXECUTIVE_RECOMMENDATION_PROPOSED');
  });

  test('retirement:ts-statements drops 13 -> 12', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBe(12);
    expect(total).toBeLessThan(13);
  });
});
