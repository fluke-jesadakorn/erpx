import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
vi.mock('server-only', () => ({}));
import { attachmentArrayFact, attachmentFactSelect, attachmentPointFact } from '../../../packages/server-benefits/src/adapters/attachment-read-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-benefits/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');

// Byte oracles captured at HEAD before the r27 fold. The byte-identical head
// `'<key>',(SELECT <projection> FROM <table> <alias> WHERE <predicate>)`
// was duplicated across the point and array related-fact renderers; both now
// route through the owner-local attachmentFactSelect renderer. Rendered SQL,
// bind order and source locks must be unchanged.
const dependentJsonArray = "dependent.id IN (SELECT jsonb_array_elements_text(CASE WHEN record.dependent_ids_json IS JSON ARRAY THEN record.dependent_ids_json::jsonb ELSE '[]'::jsonb END))";
const preDedupe = {
  plan: "'plan',(SELECT to_jsonb(plan) FROM benefit_plans plan WHERE plan.id=record.plan_id AND plan.company_id=record.company_id)",
  window: "'window',(SELECT to_jsonb(enrollment) FROM benefit_enrollment_windows enrollment WHERE enrollment.id=record.window_id AND enrollment.company_id=record.company_id)",
  dependents: "'dependents',(SELECT jsonb_agg(to_jsonb(dependent) ORDER BY dependent.id) FROM benefit_dependents dependent WHERE dependent.company_id=record.company_id AND " + dependentJsonArray + ")",
  payroll: "'payroll',(SELECT jsonb_agg(to_jsonb(input) ORDER BY input.id) FROM benefit_payroll_inputs input WHERE input.company_id=record.company_id AND input.election_id=record.id)",
} as const;
const renderings = [
  ['plan point', attachmentPointFact('plan', 'plan', 'benefit_plans', 'plan_id'), preDedupe.plan, '522cc1ad8d3858cdbe3892bc5291d18e9e34756e67f7ed98918be0e27b2c1242'],
  ['window point', attachmentPointFact('window', 'enrollment', 'benefit_enrollment_windows', 'window_id'), preDedupe.window, 'b3b7665cdac5c6e3ddf3e62528b967fa5269589c2752f7679a343c3a1210dd48'],
  ['dependent array', attachmentArrayFact('dependents', 'dependent', 'benefit_dependents', dependentJsonArray), preDedupe.dependents, '69347216a8aa20f51b3791e14af232b9fb714d1226394a66cc590c17d25f86ee'],
  ['payroll array', attachmentArrayFact('payroll', 'input', 'benefit_payroll_inputs', 'input.election_id=record.id'), preDedupe.payroll, '574beaaafbf3d7fb7d7f6ec65af949514117eda2c5458e6616b69b00d5e876c6'],
] as const;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

describe('benefits read-tail r27 related-fact head dedupe', () => {
  test.each(renderings)('%s rendering is byte-identical to the pre-dedupe literal', (_name, rendered, before, digest) => {
    expect(rendered).toBe(before);
    expect(sha(rendered)).toBe(digest);
    expect(Buffer.from(rendered)).toEqual(Buffer.from(before));
    readOnly(rendered);
  });

  test('the shared renderer reproduces both pre-dedupe head families byte-for-byte', () => {
    expect(attachmentFactSelect('plan', 'to_jsonb(plan)', 'benefit_plans', 'plan', 'plan.id=record.plan_id AND plan.company_id=record.company_id')).toBe(preDedupe.plan);
    expect(attachmentFactSelect('payroll', 'jsonb_agg(to_jsonb(input) ORDER BY input.id)', 'benefit_payroll_inputs', 'input', 'input.company_id=record.company_id AND input.election_id=record.id')).toBe(preDedupe.payroll);
    expect(sha(attachmentFactSelect('plan', 'to_jsonb(plan)', 'benefit_plans', 'plan', 'plan.id=record.plan_id AND plan.company_id=record.company_id'))).toBe('522cc1ad8d3858cdbe3892bc5291d18e9e34756e67f7ed98918be0e27b2c1242');
  });

  test('the full election related block and prepared statement keep their exact bytes', () => {
    const related = `,
 ${attachmentPointFact('plan', 'plan', 'benefit_plans', 'plan_id')},
 ${attachmentPointFact('window', 'enrollment', 'benefit_enrollment_windows', 'window_id')},
 ${attachmentArrayFact('dependents', 'dependent', 'benefit_dependents', dependentJsonArray)},
 ${attachmentArrayFact('payroll', 'input', 'benefit_payroll_inputs', 'input.election_id=record.id')}`;
    expect(sha(related)).toBe('2bb5c0f279bcc5a835842f3d4090ba46acb682dd4beed5866f093213c3af52ce');
    const statement = `SELECT record.person_id person_id,jsonb_build_object('record',to_jsonb(record)${related})::text image FROM benefit_elections record WHERE record.id=? AND record.company_id=?`;
    expect(sha(statement)).toBe('497b79e96ad01702c91af7cee3c2142aa2947d4f5ebbddf69ba21683fa1859d4');
    expect(statement.match(/\?/g)).toHaveLength(2);
    expect(statement.match(/SELECT/g)).toHaveLength(6);
    readOnly(statement);
  });

  test('the renderer owns the single head and both shape wrappers delegate to it', () => {
    const text = src('attachment-read-sources.ts');
    expect(text).toContain('export const attachmentFactSelect=(key:string,projection:string,table:string,alias:string,predicate:string)=>');
    expect(text).toContain("`'${key}',(SELECT ${projection} FROM ${table} ${alias} WHERE ${predicate})`");
    expect(text.match(/SELECT /g)).toHaveLength(3);
    const lines = text.split('\n');
    for (const name of ['attachmentFactSelect', 'attachmentPointFact', 'attachmentArrayFact']) {
      const line = lines.find(value => value.startsWith(`export const ${name}=`));
      expect(line, name).toBeDefined();
      expect(line, name).toContain(name === 'attachmentFactSelect' ? '(SELECT ' : 'attachmentFactSelect(');
    }
    expect(text).toContain("attachmentPointFact=(key:string,alias:string,table:string,fk:string)=>attachmentFactSelect(key,`to_jsonb(${alias})`,table,alias,`${alias}.id=record.${fk} AND ${alias}.company_id=record.company_id`)");
    expect(text).toContain("attachmentArrayFact=(key:string,alias:string,table:string,predicate:string)=>attachmentFactSelect(key,`jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id)`,table,alias,`${alias}.company_id=record.company_id AND ${predicate}`)");
  });

  test('the fold adds no write, row-lock, native registry or cross-owner consumer', () => {
    expect(src('attachment-read-sources.ts')).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    const text = src('attachment-read-sources.ts');
    expect(text).not.toContain('@erpx/server-benefits/adapters/benefits-native-read');
    expect(text).not.toContain('native_');
    for (const [name, count] of [
      ['purchases.ts', 1],
      ['package-obligations.ts', 1],
      ['people-package-obligations.ts', 1],
    ] as const) {
      expect(src(name).match(/LOCK\s+TABLE/g) ?? [], name).toHaveLength(count);
    }
    expect(src('purchases.ts')).toContain('IN SHARE ROW EXCLUSIVE MODE');
    expect(src('package-obligations.ts')).toContain('IN SHARE MODE');
  });

  test('retirement:ts-statements is 30, strictly below the r27 BEFORE=33 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(30);
    expect(total).toBeLessThan(33);
  });
});
