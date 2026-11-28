import {describe,expect,test} from 'vitest';
import {readFileSync} from 'node:fs';
import {
  PP_RD15_FINAL_PAY_VALID_SQL,
  PP_RD15_PEOPLE_SNAPSHOT_SQL,
} from '../../../modules/people-payroll/server/src/adapters/pp-read-tail-r15-sources';

const src = (name: string) => readFileSync(new URL(`../../../modules/people-payroll/server/src/adapters/${name}`, import.meta.url), 'utf8');
const ALL = [PP_RD15_FINAL_PAY_VALID_SQL, PP_RD15_PEOPLE_SNAPSHOT_SQL];

describe('People Payroll read-tail r15 byte-identical dedupe', () => {
  test('canonical constants own the exact read-only literals', () => {
    expect(PP_RD15_FINAL_PAY_VALID_SQL).toBe('SELECT hr_final_pay_cancellation_valid(?) valid');
    expect(PP_RD15_PEOPLE_SNAPSHOT_SQL).toBe('SELECT id,company_id,user_id,status,version,merged_into_id FROM hr_people');
    for (const sql of ALL) {
      expect(sql.toUpperCase()).toMatch(/^SELECT /);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('each duplicate family has one owner literal and composed callers', () => {
    const families = [
      {constant:'PP_RD15_FINAL_PAY_VALID_SQL', literal:PP_RD15_FINAL_PAY_VALID_SQL, callers:['final-pay-cancellation.ts']},
      {constant:'PP_RD15_PEOPLE_SNAPSHOT_SQL', literal:PP_RD15_PEOPLE_SNAPSHOT_SQL, callers:['package-obligations.ts','benefits-owner-facts.ts']},
    ];
    for (const f of families) {
      for (const name of f.callers) {
        const s = src(name);
        expect(s, name).toContain(f.constant);
        expect(s, name).not.toContain(f.literal);
      }
    }
  });

  test('the shared module declares each literal exactly once', () => {
    const s = src('pp-read-tail-r15-sources.ts');
    for (const literal of ALL) expect(s.split(literal)).toHaveLength(2);
  });
});
