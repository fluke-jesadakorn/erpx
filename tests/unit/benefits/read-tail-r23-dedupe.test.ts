import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { benefitElectionFactPrefix, benefitLearningFactArm, benefitSourceProbe, benefitWindowFactPrefix, benefitDependentFactPrefix } from '../../../packages/server-benefits/src/adapters/benefits-read-sql';
import { nativeSourceArm, nativeSourcesProbe } from '../../../packages/server-benefits/src/adapters/record-purchases';

const srcRoot = resolve(__dirname, '../../../packages/server-benefits/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Pre-dedupe literal, copied byte-for-byte from HEAD. The attachment file-source
// probe and the native-source probe each rendered this exact wrapper over a
// different source expression; one owner-local builder now renders both.
const preDedupe = (source: string, alias: string) => `SELECT 1 FROM (${source}) ${alias}`;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

describe('Benefits read-tail r23 parameterized source-probe family', () => {
  test('builder reproduces both pre-dedupe probe SELECTs byte-for-byte', () => {
    for (const source of ['(native.sources)', '(people.engagements)', 'benefit_file_source']) {
      for (const alias of ['benefit_file_source', 'benefit_actual', 'benefit_source']) {
        expect(benefitSourceProbe(source, alias)).toBe(preDedupe(source, alias));
        readOnly(preDedupe(source, alias));
      }
    }
  });

  test('r14 native-source probe exports the same unchanged bytes', () => {
    // Guard against regressing the earlier r14 contract when it became a caller.
    const nativeSources = [
      nativeSourceArm('plan', 'benefit_plans', true),
      nativeSourceArm('window', 'benefit_enrollment_windows'),
      nativeSourceArm('dependent', 'benefit_dependents', false, true),
      nativeSourceArm('election', 'benefit_elections', false, true, ' JOIN benefit_plans plan ON plan.id=r.plan_id AND plan.company_id=r.company_id'),
    ].join('\n UNION ALL ');
    for (const alias of ['benefit_actual', 'benefit_source']) {
      expect(nativeSourcesProbe(alias)).toBe(preDedupe(nativeSources, alias));
    }
  });

  test('one builder definition and one caller in each consumer', () => {
    expect(src('benefits-read-sql.ts').match(/benefitSourceProbe\s*=/g) ?? []).toHaveLength(1);
    const attach = src('attachment-purchases.ts');
    expect(attach).not.toContain('`SELECT 1 FROM (${source(type)}) benefit_file_source WHERE`');
    expect(attach.match(/benefitSourceProbe\(/g) ?? []).toHaveLength(2);
    const record = src('record-purchases.ts');
    expect(record).not.toContain('`SELECT 1 FROM (${nativeSources}) ${alias}`');
    expect(record.match(/benefitSourceProbe\(/g) ?? []).toHaveLength(2);
  });

  test('the new family adds no write or lock statement', () => {
    expect(src('benefits-read-sql.ts')).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    // Pre-existing source locks in the consumers are untouched.
    expect(src('attachment-purchases.ts').match(/LOCK\s+TABLE/g) ?? []).toHaveLength(0);
    expect(src('record-purchases.ts').match(/LOCK\s+TABLE/g) ?? []).toHaveLength(0);
  });
});

describe('Benefits read-tail r23 UNION fact-arm family', () => {
  // Pre-dedupe literals, copied byte-for-byte from HEAD, used only as the oracle.
  const preDedupe = {
    election: "UNION ALL SELECT 'election',e.id,e.version,e.status,to_jsonb(e),e.status NOT IN ('released','cancelled') OR",
    window: "UNION ALL SELECT 'window',w.id,w.version,w.status,to_jsonb(w),",
    dependent: "UNION ALL SELECT 'dependent',d.id,d.version,d.status,to_jsonb(d),d.status='pending'",
  } as const;

  test('generator reproduces all three pre-dedupe UNION arms byte-for-byte', () => {
    expect(benefitElectionFactPrefix).toBe(preDedupe.election);
    expect(benefitWindowFactPrefix).toBe(preDedupe.window);
    expect(benefitDependentFactPrefix).toBe(preDedupe.dependent);
    for (const sql of Object.values(preDedupe)) readOnly(sql);
    expect(benefitLearningFactArm('election', 'e.id', 'e.version', 'e.status', 'to_jsonb(e)', "e.status NOT IN ('released','cancelled') OR")).toBe(preDedupe.election);
    expect(benefitLearningFactArm('window', 'w.id', 'w.version', 'w.status', 'to_jsonb(w)', '')).toBe(preDedupe.window);
    expect(benefitLearningFactArm('dependent', 'd.id', 'd.version', 'd.status', 'to_jsonb(d)', "d.status='pending'")).toBe(preDedupe.dependent);
  });

  test('one owner generator and three composed callers; no raw arm literals remain', () => {
    const sql = src('benefits-read-sql.ts');
    expect(sql.match(/export const benefitLearningFactArm\s*=/g) ?? []).toHaveLength(1);
    expect(sql.match(/benefitLearningFactArm\(/g) ?? []).toHaveLength(3);
    for (const literal of Object.values(preDedupe)) expect(sql).not.toContain(literal);
  });

  test('the new family adds no write or lock statement', () => {
    expect(src('benefits-read-sql.ts')).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    expect(src('package-obligations.ts').match(/LOCK\s+TABLE/g) ?? []).toHaveLength(1);
    expect(src('people-package-obligations.ts').match(/LOCK\s+TABLE/g) ?? []).toHaveLength(1);
  });
});

describe('Benefits read-tail r23 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r23 BEFORE=40 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(40);
  });
});
