import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { benefitAttachmentSourceSelect } from '../../../packages/server-benefits/src/adapters/benefits-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-benefits/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Pre-dedupe literals, copied from HEAD, used only as the byte-identity oracle.
// $TABLE/$PEOPLE/$ENGAGEMENT stand for caller-supplied interpolations.
const preDedupe = {
  plan: `SELECT r.company_id,NULL::text branch_id,NULL::text department_id,r.status,NULL::text person_user_id,(to_jsonb(r)-'version'-'updated_at')::text image FROM benefit_plans r WHERE r.id=? AND r.company_id=?`,
  person: `SELECT r.company_id,e.branch_id,e.department_id,r.status,p.user_id person_user_id,jsonb_build_object('record',to_jsonb(r)-'version'-'updated_at','person',to_jsonb(p),'engagement',to_jsonb(e))::text image FROM $TABLE r JOIN ($PEOPLE) p ON p.id=r.person_id AND p.company_id=r.company_id $ENGAGEMENT WHERE r.id=? AND r.company_id=?`,
} as const;

const personFrom = `$TABLE r JOIN ($PEOPLE) p ON p.id=r.person_id AND p.company_id=r.company_id $ENGAGEMENT`;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

describe('Benefits read-tail r21 parameterized attachment source projection family', () => {
  test('generator reproduces both pre-dedupe source SELECTs byte-for-byte', () => {
    expect(benefitAttachmentSourceSelect('NULL::text branch_id', 'NULL::text department_id', 'NULL::text person_user_id', "(to_jsonb(r)-'version'-'updated_at')::text", 'benefit_plans r')).toBe(preDedupe.plan);
    expect(benefitAttachmentSourceSelect('e.branch_id', 'e.department_id', 'p.user_id person_user_id', "jsonb_build_object('record',to_jsonb(r)-'version'-'updated_at','person',to_jsonb(p),'engagement',to_jsonb(e))::text", personFrom)).toBe(preDedupe.person);
    readOnly(preDedupe.plan);
    readOnly(preDedupe.person);
  });

  test('generator has one owner literal and two composed callers', () => {
    expect(src('benefits-read-sql.ts').match(/benefitAttachmentSourceSelect/g) ?? []).toHaveLength(1);
    const purchases = src('attachment-purchases.ts');
    expect(purchases).not.toContain('SELECT r.company_id,NULL::text branch_id');
    expect(purchases).not.toContain('SELECT r.company_id,e.branch_id,e.department_id');
    expect(purchases.match(/benefitAttachmentSourceSelect\(/g) ?? []).toHaveLength(2);
  });

  test('the generator family adds no write or lock statement', () => {
    for (const name of ['benefits-read-sql.ts', 'attachment-purchases.ts']) {
      expect(src(name), name).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
  });
});

describe('Benefits read-tail r21 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r21 BEFORE=43 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(43);
  });
});
