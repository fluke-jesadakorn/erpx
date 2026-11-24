import {describe,expect,test} from 'vitest';
import {readFileSync} from 'node:fs';
import {receiptProbe} from '../../../packages/server-commercial/src/adapters/commercial-read-tail-r5-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-commercial/src/adapters/${name}`, import.meta.url), 'utf8');
const LITERAL = "SELECT 1 FROM company_package_change_sets WHERE id=?";
const PROBE = 'receiptProbe("company_package_change_sets","1")';

describe('Commercial read-tail r1 retired into the r5 receipt probe', () => {
  test('the r5 probe reproduces the former r1 constant byte-identically', () => {
    expect(receiptProbe('company_package_change_sets', '1')).toBe(LITERAL);
    expect(receiptProbe('company_package_change_sets', '1').toUpperCase()).toMatch(/^SELECT /);
    expect(receiptProbe('company_package_change_sets', '1')).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('each former site prepares the delegated probe with one unchanged bind', () => {
    const pg = src('package-grants.ts'), lc = src('company-package-lifecycle.ts');
    expect(pg).not.toContain('commercialChangeSetReceiptSql');
    expect(lc).not.toContain('commercialChangeSetReceiptSql');
    expect(pg.split(PROBE)).toHaveLength(2);
    expect(lc.split(PROBE)).toHaveLength(3);
    expect(lc.split('.bind(input.idempotencyKey)')).toHaveLength(3);
    expect(pg).toContain(`.bind(key).first()) throw new Error("IDEMPOTENCY_CONFLICT")`);
    for (const name of ['package-grants.ts', 'company-package-lifecycle.ts']) expect(src(name), name).not.toContain(LITERAL);
  });

  test('the r1 module is gone and the native read plan names the probe caller', () => {
    expect(() => src('commercial-read-tail-r1-sources.ts')).toThrow();
    expect(src('../public/native_read.rs')).toContain('caller: "commercial-read-tail-r5-sources.receiptProbe"');
  });
});
