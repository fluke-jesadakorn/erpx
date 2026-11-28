import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { benefitMetricCountSql } from '../../../modules/benefits/server/src/adapters/benefits-read-sql';

const srcRoot = resolve(__dirname, '../../../modules/benefits/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Pre-dedupe literals, copied from HEAD, used only as the byte-identity oracle.
// $SCOPE stands for the caller-supplied ${scope.where} interpolation.
const scope = '$SCOPE';
const preDedupe = {
  elections: `SELECT e.status label,count(*)::text value FROM benefit_elections e WHERE e.company_id=? AND ${scope} AND e.status<>'cancelled' AND e.effective_from<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.effective_to IS NULL OR e.effective_to>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) GROUP BY e.status ORDER BY e.status`,
  plans: `SELECT 'Published plans' label,count(*)::text value FROM benefit_plans WHERE company_id=? AND ${scope} AND status='published' AND effective_from<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (effective_to IS NULL OR effective_to>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))`,
} as const;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

describe('Benefits read-tail r17 parameterized executive metric family', () => {
  test('generator reproduces both pre-dedupe metric SELECTs byte-for-byte', () => {
    expect(benefitMetricCountSql('e.status', 'benefit_elections', 'e', "status<>'cancelled'", ' GROUP BY e.status ORDER BY e.status', scope)).toBe(preDedupe.elections);
    expect(benefitMetricCountSql("'Published plans'", 'benefit_plans', '', "status='published'", '', scope)).toBe(preDedupe.plans);
    readOnly(preDedupe.elections);
    readOnly(preDedupe.plans);
  });

  test('generator has one owner literal and two composed metric callers', () => {
    expect(src('benefits-read-sql.ts').match(/benefitMetricCountSql/g) ?? []).toHaveLength(1);
    const text = src('executive-metric-projections.ts');
    expect(text).not.toContain('count(*)::text value');
    expect(text.match(/sql = benefitMetricCountSql\(/g) ?? []).toHaveLength(2);
  });

  test('the generator family adds no write or lock statement', () => {
    expect(src('benefits-read-sql.ts')).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    expect(src('executive-metric-projections.ts')).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
});

describe('Benefits read-tail r17 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r17 BEFORE=47 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(47);
  });
});
