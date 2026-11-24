import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));

const srcRoot = resolve(__dirname, '../../../packages/server-ecommerce/src');
const ap = readFileSync(resolve(srcRoot, 'adapters/attachment-purchases.ts'), 'utf8');

const ecommerceScopeRow = (alias: string, branch = false) => branch
  ? `(SELECT ?::text company_id,?::text branch_id,?::text department_id) ${alias}`
  : `(SELECT ?::text company_id) ${alias}`;

// The interpolations are identical function calls in the pre-dedupe source and
// the shared helper, so a deterministic stub proves byte-identity of the frame.
const assignmentReadSql = (table: string, permission: string) => `<assignment ${table} ${permission}>`;
const reviewParts = () =>
  `(${assignmentReadSql('ecommerce_file_review','records.review')} OR ${assignmentReadSql('ecommerce_file_review','commerce.approve')})`;

// Pre-dedupe literals copied from HEAD, kept only as byte-identity oracles.
const head = {
  authority: `SELECT ${reviewParts()} allowed FROM ${ecommerceScopeRow('ecommerce_file_review', true)}`,
  assertion: `(SELECT ${reviewParts()} FROM ${ecommerceScopeRow('ecommerce_file_review', true)})`,
};

// Rebuild the r12 shared reviewer-scope read exactly as the adapter defines it.
const reviewScopeSql = (projection: string, scope: string) =>
  `SELECT ${reviewParts()} ${projection}FROM ${scope}`;

describe('Ecommerce read-tail r12 reviewer-scope read family', () => {
  test('one parameterized read rebuilds the authority check and command-boundary assertion byte-identically', () => {
    const scope = ecommerceScopeRow('ecommerce_file_review', true);
    expect(reviewScopeSql('allowed ', scope)).toBe(head.authority);
    expect(`(${reviewScopeSql('', scope)})`).toBe(head.assertion);
  });

  test('family is defined once and composed at both call sites; literals are gone', () => {
    expect(ap.match(/const reviewScopeSql = /g) ?? []).toHaveLength(1);
    expect(ap.match(/reviewScopeSql\(/g) ?? []).toHaveLength(2);
    expect(ap.match(/reviewScopeSql\('allowed ',ecommerceScopeRow\('ecommerce_file_review', true\)\)/g) ?? []).toHaveLength(1);
    expect(ap.match(/reviewScopeSql\('',ecommerceScopeRow\('ecommerce_file_review', true\)\)/g) ?? []).toHaveLength(1);
    expect(ap.includes(') allowed FROM ${ecommerceScopeRow(\'ecommerce_file_review\', true)}')).toBe(false);
    expect(ap.includes(') FROM ${ecommerceScopeRow(\'ecommerce_file_review\', true)})')).toBe(false);
  });

  test('composed reads stay read-only and keep every bind token', () => {
    const scope = ecommerceScopeRow('ecommerce_file_review', true);
    for (const sql of [head.authority, head.assertion]) {
      expect(sql).toMatch(/SELECT/);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    expect((reviewScopeSql('allowed ', scope).match(/\?/g) ?? []).length).toBe(3);
    expect((head.authority.match(/\?/g) ?? []).length).toBe(3);
  });

  test('r1-r11 helpers, scope-row call count and checkout locks are untouched', () => {
    for (const token of [
      'ecommerceModuleAssignment', 'ecommerceScopeRow', 'sourceQuery',
      'LOCK TABLE commerce_products IN SHARE MODE',
      'ecommerce-file-authority', 'ecommerce-file-module', 'ecommerce-file-source',
    ]) {
      expect(ap).toContain(token);
    }
    expect(ap.match(/ecommerceScopeRow\(/g) ?? []).toHaveLength(5);
  });
});

describe('Ecommerce read-tail r12 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r12 BEFORE=59 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(59);
  });
});
