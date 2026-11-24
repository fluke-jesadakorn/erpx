import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { benefitLearningFactArm } from '../../../packages/server-benefits/src/adapters/benefits-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-benefits/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Pre-dedupe literals, copied from HEAD, used only as the byte-identity oracle.
// $CONSUMPTIONS stands for the caller-supplied ${learning.consumptions} interpolation.
const consumptionSource = '$CONSUMPTIONS';
const preDedupe = {
  consumption: `UNION ALL SELECT 'learning-consumption',c.id,c.source_assignment_version,'consumed',c.image,
 e.id IS NULL OR c.person_id IS DISTINCT FROM e.person_id OR c.certificate_id IS DISTINCT FROM e.learning_certificate_id OR c.certificate_source_id IS NULL OR c.certificate_company_id IS DISTINCT FROM c.company_id OR c.certificate_person_id IS DISTINCT FROM c.person_id OR c.certificate_handoff_id IS DISTINCT FROM c.handoff_id OR c.certificate_source_version IS DISTINCT FROM c.source_assignment_version OR c.certificate_code IS DISTINCT FROM c.eligibility_code
 OR c.handoff_source_id IS NULL OR c.handoff_company_id IS DISTINCT FROM c.company_id OR c.handoff_person_id IS DISTINCT FROM c.person_id OR c.handoff_assignment_id IS DISTINCT FROM c.assignment_id OR c.handoff_source_version IS DISTINCT FROM c.source_assignment_version OR c.handoff_code IS DISTINCT FROM c.eligibility_code OR c.assignment_source_id IS NULL OR c.assignment_company_id IS DISTINCT FROM c.company_id OR c.assignment_version<c.source_assignment_version
 FROM (${consumptionSource}) c CROSS JOIN scope LEFT JOIN elections e ON e.id=c.election_id WHERE c.company_id=scope.company_id
`,
  missing: `UNION ALL SELECT 'missing-learning-consumption',e.id,e.version,e.status,to_jsonb(e),true FROM elections e JOIN plans plan ON plan.id=e.plan_id WHERE e.status<>'cancelled' AND plan.required_eligibility_code IS NOT NULL AND NOT EXISTS(SELECT 1 FROM (${consumptionSource}) c WHERE c.election_id=e.id AND c.company_id=e.company_id AND c.certificate_id=e.learning_certificate_id AND c.person_id=e.person_id AND c.eligibility_code=plan.required_eligibility_code)`,
} as const;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

describe('Benefits read-tail r19 parameterized learning fact arm family', () => {
  test('generator reproduces both pre-dedupe learning UNION arms byte-for-byte', () => {
    const consumptionTail = preDedupe.consumption.slice(preDedupe.consumption.indexOf('c.image,') + 'c.image,'.length);
    const missingTail = preDedupe.missing.slice(preDedupe.missing.indexOf('to_jsonb(e),') + 'to_jsonb(e),'.length);
    expect(benefitLearningFactArm('learning-consumption', 'c.id', 'c.source_assignment_version', "'consumed'", 'c.image', consumptionTail)).toBe(preDedupe.consumption);
    expect(benefitLearningFactArm('missing-learning-consumption', 'e.id', 'e.version', 'e.status', 'to_jsonb(e)', missingTail)).toBe(preDedupe.missing);
    readOnly(preDedupe.consumption);
    readOnly(preDedupe.missing);
  });

  test('generator has one owner literal and two composed callers', () => {
    expect(src('benefits-read-sql.ts').match(/benefitLearningFactArm/g) ?? []).toHaveLength(1);
    const text = src('package-obligations.ts');
    expect(text).not.toContain("UNION ALL SELECT 'learning-consumption',");
    expect(text).not.toContain("UNION ALL SELECT 'missing-learning-consumption',");
    expect(text.match(/benefitLearningFactArm\(/g) ?? []).toHaveLength(2);
  });

  test('the generator family adds no write or lock statement', () => {
    expect(src('benefits-read-sql.ts')).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
});

describe('Benefits read-tail r19 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r19 BEFORE=45 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(45);
  });
});
