import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { attachmentArrayFact, attachmentPointFact } from '../../../packages/server-benefits/src/adapters/attachment-read-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-benefits/src');
const src = readFileSync(resolve(srcRoot, 'adapters/attachment-read-sources.ts'), 'utf8');

// Pre-dedupe literals, copied from HEAD, used only as the byte-identity oracle.
const dependentJsonArray = "dependent.id IN (SELECT jsonb_array_elements_text(CASE WHEN record.dependent_ids_json IS JSON ARRAY THEN record.dependent_ids_json::jsonb ELSE '[]'::jsonb END))";
const old = {
  plan: "'plan',(SELECT to_jsonb(plan) FROM benefit_plans plan WHERE plan.id=record.plan_id AND plan.company_id=record.company_id)",
  window: "'window',(SELECT to_jsonb(enrollment) FROM benefit_enrollment_windows enrollment WHERE enrollment.id=record.window_id AND enrollment.company_id=record.company_id)",
  dependents: "'dependents',(SELECT jsonb_agg(to_jsonb(dependent) ORDER BY dependent.id) FROM benefit_dependents dependent WHERE dependent.company_id=record.company_id AND " + dependentJsonArray + ")",
  payroll: "'payroll',(SELECT jsonb_agg(to_jsonb(input) ORDER BY input.id) FROM benefit_payroll_inputs input WHERE input.company_id=record.company_id AND input.election_id=record.id)",
} as const;
const preDedupeRelated = `,
 ${old.plan},
 ${old.window},
 ${old.dependents},
 ${old.payroll}`;

describe('Benefits read-tail r15 election related-fact family', () => {
  test.each([
    ['plan', 'plan', 'benefit_plans', 'plan_id', old.plan],
    ['window', 'enrollment', 'benefit_enrollment_windows', 'window_id', old.window],
  ] as const)('point fact %s stays byte-identical to the pre-dedupe literal', (key, alias, table, fk, expected) => {
    expect(attachmentPointFact(key, alias, table, fk)).toBe(expected);
  });

  test.each([
    ['dependents', 'dependent', 'benefit_dependents', dependentJsonArray, old.dependents],
    ['payroll', 'input', 'benefit_payroll_inputs', 'input.election_id=record.id', old.payroll],
  ] as const)('array fact %s stays byte-identical to the pre-dedupe literal', (key, alias, table, predicate, expected) => {
    expect(attachmentArrayFact(key, alias, table, predicate)).toBe(expected);
  });

  test('the whole election related block stays byte-identical', () => {
    const generated = `,
 ${attachmentPointFact('plan', 'plan', 'benefit_plans', 'plan_id')},
 ${attachmentPointFact('window', 'enrollment', 'benefit_enrollment_windows', 'window_id')},
 ${attachmentArrayFact('dependents', 'dependent', 'benefit_dependents', dependentJsonArray)},
 ${attachmentArrayFact('payroll', 'input', 'benefit_payroll_inputs', 'input.election_id=record.id')}`;
    expect(generated).toBe(preDedupeRelated);
    expect(generated).toContain('SELECT ');
    expect(generated).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the family is one generator per shape with all callers and no inlining', () => {
    expect(src.match(/export const attachmentPointFact=/g) ?? []).toHaveLength(1);
    expect(src.match(/export const attachmentArrayFact=/g) ?? []).toHaveLength(1);
    expect(src.match(/attachmentPointFact\('/g) ?? []).toHaveLength(2);
    expect(src.match(/attachmentArrayFact\('/g) ?? []).toHaveLength(2);
    expect(src).not.toMatch(/\(SELECT to_jsonb\(plan\) FROM benefit_plans plan/);
    expect(src).not.toMatch(/\(SELECT to_jsonb\(enrollment\) FROM benefit_enrollment_windows enrollment/);
    expect(src).not.toMatch(/\(SELECT jsonb_agg\(to_jsonb\(dependent\)/);
    expect(src).not.toMatch(/\(SELECT jsonb_agg\(to_jsonb\(input\)/);
    const family = src.slice(src.indexOf('export const attachmentPointFact='), src.indexOf('export async function benefitAttachmentReadImage'));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
});

describe('Benefits read-tail r15 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r15 BEFORE=50 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(50);
  });
});
