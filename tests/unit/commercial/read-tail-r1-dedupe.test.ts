import {describe,expect,test} from 'vitest';
import {readFileSync} from 'node:fs';
import {commercialChangeSetReceiptSql} from '../../../packages/server-commercial/src/adapters/commercial-read-tail-r1-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-commercial/src/adapters/${name}`, import.meta.url), 'utf8');
const LITERAL = "SELECT 1 FROM company_package_change_sets WHERE id=?";
const CALLERS = ['company-package-lifecycle.ts', 'package-grants.ts'];

describe('Commercial read-tail r1 byte-identical dedupe', () => {
  test('the canonical constant owns the exact pure-read literal', () => {
    expect(commercialChangeSetReceiptSql).toBe(LITERAL);
    expect(commercialChangeSetReceiptSql.toUpperCase()).toMatch(/^SELECT /);
    expect(commercialChangeSetReceiptSql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('each former duplicate site prepares the shared constant independently', () => {
    for (const name of CALLERS) {
      const s = src(name);
      expect(s, name).toContain('commercialChangeSetReceiptSql');
      expect(s, name).not.toContain(LITERAL);
    }
    expect(src('company-package-lifecycle.ts').split('prepare(commercialChangeSetReceiptSql)')).toHaveLength(3);
    expect(src('package-grants.ts').split('prepare(commercialChangeSetReceiptSql)')).toHaveLength(2);
  });

  test('the shared module declares the literal exactly once', () => {
    expect(src('commercial-read-tail-r1-sources.ts').split(LITERAL)).toHaveLength(2);
  });
});
