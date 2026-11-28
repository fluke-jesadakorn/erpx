import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

const srcRoot = resolve(__dirname, '../../../modules/planning/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const pub = (name: string) => readFileSync(resolve(srcRoot, 'public', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const norm = (text: string) => text.replace(/\s+/g, '');

/** The registry statement with its `?` markers in native positional form. */
const nativeBinds = (sql: string) =>
  sql.split('?').map((part, index) => (index === 0 ? part : `$${index}${part}`)).join('');

describe('Planning r26 scenarios-list native cutover', () => {
  test('scenarios is served by the owner route and emits no TS SQL', () => {
    const caller = src('production-command-sources-read.ts');
    expect(caller).toContain('nativeScenarios(companyId,branchId,branchId2)');
    expect(caller.match(/planningScopedBranchList\(/g) ?? []).toHaveLength(0);
    expect(caller).not.toContain('p.*,s.name specification_name');
    expect(caller).not.toMatch(/prepare\(`?\s*SELECT p\.\*/);
  });

  test('the TS transport admits scenarios as a closed list kind and validates its exact row', () => {
    const client = src('native-lane-36/client.ts');
    expect(client).toContain("| 'scenarios'");
    expect(client).toContain("'recommendations', 'scenarios', 'resources'");
    expect(client).toContain('export const nativeScenarios = scoped(\'scenarios\')');
    expect(client).toContain('const SCENARIO_LIST_KEYS = [');
    expect(client).toContain("'specification_name',");
    expect(client).toContain("if (kind === 'scenarios')");
    expect(client.match(/db\.prepare/g) ?? []).toHaveLength(0);
  });

  test('the native owner statement is the byte-exact registered scenarios plan', () => {
    const registry = pub('read_source.rs');
    const declared = registry.match(/const SQL_SCENARIOS: &str = concat!\(([^;]*)\);/)?.[1] ?? '';
    const registered = (declared.match(/"([^"]*)"/g) ?? []).map(part => part.slice(1, -1)).join('');
    expect(registered).toContain('SELECT p.*,s.name specification_name');
    const postgres = src('native_lane_36/postgres.rs');
    expect(norm(postgres)).toContain(norm(nativeBinds(registered)));
    expect(postgres).toContain('pub const SCENARIOS_SQL: &str');
    expect(postgres).toContain('"scenarios" => Some(SCENARIOS_SQL)');
  });

  test('the registry binds the retired kind to the native statement oracle', () => {
    const registry = pub('read_source.rs');
    expect(registry).toContain('include_str!("../adapters/native_lane_36/postgres.rs")');
    expect(registry).toContain('if entry.kind == RD_SCENARIOS');
    expect(registry).toContain('native_binds(entry.sql)');
  });

  test('retirement:ts-statements drops to 12 after r27', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r27 retired the shared planning-select-from-sql.ts head; the remaining
    // single-line builder still serves the transaction-bound calendar tail.
    expect(total).toBe(5);
    expect(total).toBeLessThan(13);
  });
});
