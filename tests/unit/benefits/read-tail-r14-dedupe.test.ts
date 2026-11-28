import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { benefitAttachmentPeopleSources as people } from '@erpx/people-payroll-server/public/benefits-owner-facts';
import { nativeSourceArm, nativeSourcesProbe } from '../../../modules/benefits/server/src/adapters/record-purchases';

const srcRoot = resolve(__dirname, '../../../modules/benefits/server/src');
const src = readFileSync(resolve(srcRoot, 'adapters/record-purchases.ts'), 'utf8');

// Pre-dedupe literals, copied from HEAD, used only as the byte-identity oracle.
const activeEngagementScope = `EXISTS(SELECT 1 FROM (${people.engagements}) e WHERE e.person_id=p.id AND e.company_id=p.company_id AND e.status='active')`;
const arm = (kind: string) => {
  const old = {
    plan: `SELECT r.id,r.company_id,'plan' kind,NULL::text person_user_id,TRUE current_person FROM benefit_plans r`,
    window: `SELECT r.id,r.company_id,'window',NULL::text,TRUE FROM benefit_enrollment_windows r`,
    dependent: `SELECT r.id,r.company_id,'dependent',p.user_id,p.status='active' AND p.merged_into_id IS NULL AND ${activeEngagementScope} FROM benefit_dependents r JOIN (${people.people}) p ON p.id=r.person_id AND p.company_id=r.company_id`,
    election: `SELECT r.id,r.company_id,'election',p.user_id,p.status='active' AND p.merged_into_id IS NULL AND ${activeEngagementScope} FROM benefit_elections r JOIN benefit_plans plan ON plan.id=r.plan_id AND plan.company_id=r.company_id JOIN (${people.people}) p ON p.id=r.person_id AND p.company_id=r.company_id`,
  } as const;
  return old[kind as keyof typeof old];
};
const preDedupeSql = [arm('plan'), arm('window'), arm('dependent'), arm('election')].join('\n UNION ALL ');

describe('Benefits read-tail r14 nativeSources probe family', () => {
  test.each([
    ['plan', 'benefit_plans', true, false, ''],
    ['window', 'benefit_enrollment_windows', false, false, ''],
    ['dependent', 'benefit_dependents', false, true, ''],
    ['election', 'benefit_elections', false, true, ' JOIN benefit_plans plan ON plan.id=r.plan_id AND plan.company_id=r.company_id'],
  ] as const)('%s arm stays byte-identical to the pre-dedupe literal', (kind, table, first, person, join) => {
    expect(nativeSourceArm(kind, table, first, person, join)).toBe(arm(kind));
  });

  test('the full nativeSources UNION and its probes stay byte-identical', () => {
    const generated = [
      nativeSourceArm('plan', 'benefit_plans', true),
      nativeSourceArm('window', 'benefit_enrollment_windows'),
      nativeSourceArm('dependent', 'benefit_dependents', false, true),
      nativeSourceArm('election', 'benefit_elections', false, true, ' JOIN benefit_plans plan ON plan.id=r.plan_id AND plan.company_id=r.company_id'),
    ].join('\n UNION ALL ');
    expect(generated).toBe(preDedupeSql);
    for (const alias of ['benefit_actual', 'benefit_source']) {
      expect(nativeSourcesProbe(alias)).toBe(`SELECT 1 FROM (${preDedupeSql}) ${alias}`);
    }
    expect(nativeSourcesProbe('benefit_actual')).toContain('SELECT ');
    expect(nativeSourcesProbe('benefit_source')).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the family is one arm literal with four callers and one probe with two callers, with no inlining', () => {
    expect(src.match(/export const nativeSourceArm=/g) ?? []).toHaveLength(1);
    expect(src.match(/nativeSourceArm\('/g) ?? []).toHaveLength(4);
    expect(src.match(/export const nativeSourcesProbe=/g) ?? []).toHaveLength(1);
    expect(src.match(/nativeSourcesProbe\('/g) ?? []).toHaveLength(2);
    expect(src).not.toMatch(/SELECT r\.id,r\.company_id,'(plan|window|dependent|election)'/);
    expect(src).toContain(`${'${nativeSourcesProbe(\'benefit_actual\')}'} WHERE`);
    expect(src).toContain(`${'${nativeSourcesProbe(\'benefit_source\')}'} CROSS JOIN`);
  });

  test('the generator family adds no write or lock statement', () => {
    const family = src.slice(src.indexOf('export const nativeSourceArm='), src.indexOf('function visible('));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
});

describe('Benefits read-tail r14 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r14 BEFORE=54 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(54);
  });
});
