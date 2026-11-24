import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { benefitEventHistoryArm } from '../../../packages/server-benefits/src/adapters/package-obligations';

const srcRoot = resolve(__dirname, '../../../packages/server-benefits/src');
const src = readFileSync(resolve(srcRoot, 'adapters/package-obligations.ts'), 'utf8');

// Pre-dedupe literal, copied from HEAD, used only as the byte-identity oracle.
function arm(kind: string, events: string, parent: string, fk: string, first: boolean) {
  return `${first ? '' : 'UNION ALL '}SELECT '${kind}'${first ? ' kind' : ''},to_jsonb(event)::text${first ? ' image' : ''} FROM ${events} event JOIN ${parent} r ON r.id=event.${fk} WHERE r.company_id=?`;
}
const preDedupeSql = `SELECT 'plan' kind,to_jsonb(event)::text image FROM benefit_plan_events event JOIN benefit_plans r ON r.id=event.plan_id WHERE r.company_id=? UNION ALL SELECT 'window',to_jsonb(event)::text FROM benefit_window_events event JOIN benefit_enrollment_windows r ON r.id=event.window_id WHERE r.company_id=? UNION ALL SELECT 'dependent',to_jsonb(event)::text FROM benefit_dependent_events event JOIN benefit_dependents r ON r.id=event.dependent_id WHERE r.company_id=? UNION ALL SELECT 'election',to_jsonb(event)::text FROM benefit_election_events event JOIN benefit_elections r ON r.id=event.election_id WHERE r.company_id=? ORDER BY kind,image`;

describe('Benefits read-tail r13 event-history UNION family', () => {
  test.each([
    ['plan', 'benefit_plan_events', 'benefit_plans', 'plan_id', true],
    ['window', 'benefit_window_events', 'benefit_enrollment_windows', 'window_id', false],
    ['dependent', 'benefit_dependent_events', 'benefit_dependents', 'dependent_id', false],
    ['election', 'benefit_election_events', 'benefit_elections', 'election_id', false],
  ] as const)('%s arm stays byte-identical to the pre-dedupe literal', (kind, events, parent, fk, first) => {
    expect(benefitEventHistoryArm(kind, events, parent, fk, first)).toBe(arm(kind, events, parent, fk, first));
  });

  test('the full generated history SQL stays byte-identical to the pre-dedupe literal', () => {
    const generated = [
      benefitEventHistoryArm('plan', 'benefit_plan_events', 'benefit_plans', 'plan_id', true),
      benefitEventHistoryArm('window', 'benefit_window_events', 'benefit_enrollment_windows', 'window_id', false),
      benefitEventHistoryArm('dependent', 'benefit_dependent_events', 'benefit_dependents', 'dependent_id', false),
      benefitEventHistoryArm('election', 'benefit_election_events', 'benefit_elections', 'election_id', false),
    ].join(' ') + ' ORDER BY kind,image';
    expect(generated).toBe(preDedupeSql);
  });

  test('the family is one owner literal with four callers and no write or lock', () => {
    expect(src.match(/export const benefitEventHistoryArm=/g) ?? []).toHaveLength(1);
    expect(src.match(/benefitEventHistoryArm\('/g) ?? []).toHaveLength(4);
    const family = src.slice(src.indexOf('export const benefitEventHistoryArm='), src.indexOf('export async function benefitPackageObligations'));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
});

describe('Benefits read-tail r13 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r13 BEFORE=57 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(57);
  });
});
