import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');

const KIND = 'lifecycle_read_rehire_position';
const SQL = 'SELECT * FROM organization_positions WHERE company_id=? AND id=? AND active=1';
// A separate locked statement; it is not part of this family and must not change.
const LOCKED_LITERAL = 'SELECT * FROM organization_positions WHERE company_id=$1 AND id=$2 AND active=1 FOR UPDATE';

describe('Organization active-position family retired to native owner', () => {
  test('shared r38 helper is deleted and both former consumers send the typed kind', () => {
    expect(() => read('organization-column-alias-families-r38.ts')).toThrow();
    const facts = read('people-lifecycle-facts.ts');
    const worker = read('people-lifecycle-worker-effects.ts');
    for (const source of [facts, worker]) {
      expect(source).not.toContain('activePositionById');
      expect(source).toContain(`'${KIND}'`);
    }
    expect(worker).toContain('transactionDatabase');
    expect(facts).toContain('nativeOrgOne');
  });

  test('worker keeps the pg QueryResult parity shape without a local statement', () => {
    const worker = read('people-lifecycle-worker-effects.ts');
    expect(worker).toContain('.then(rows => ({ rows, rowCount: rows.length }))');
    expect(worker).not.toContain('client.query<LifecyclePositionFacts>');
    // The FOR UPDATE lock is a separate statement and stays byte-identical.
    expect(read('people-lifecycle-facts.ts')).toContain(LOCKED_LITERAL);
  });

  test('Rust owner keeps one byte-identical read-only statement for both consumers', () => {
    const registry = read('organization_owner_write.rs');
    expect(registry).toContain(`pub const LIFECYCLE_READ_REHIRE_POSITION: &str = "${KIND}";`);
    expect(registry).toContain(`            "${SQL}"`);
  });

  test('retirement:ts-statements drops to 188 after deleting the shared helper literal', () => {
    let space = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) space += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(space).toBe(179);
  });
});
