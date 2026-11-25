import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  PP_RD24_UNCLAIMED_EXPENSE_GUARD,
  PP_RD24_EXPENSE_AI_JOB_LEASE_GUARD,
  PP_RD24_PEOPLE_PERSON_USER_SAME_SQL,
  PP_RD24_PEOPLE_PERSON_USER_OK_SQL,
  PP_RD24_TIME_ENTRY_REVIEWABLE_GUARD,
  PP_RD24_STATUTORY_PACK_GUARD,
  PP_RD24_APPROVED_RUN_EVIDENCE_GUARD,
  PP_RD24_TARGET_ID_SQL,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r24-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const quoted = (literal: string) => JSON.stringify(literal);

const literals = [
  PP_RD24_UNCLAIMED_EXPENSE_GUARD,
  PP_RD24_EXPENSE_AI_JOB_LEASE_GUARD,
  PP_RD24_PEOPLE_PERSON_USER_SAME_SQL,
  PP_RD24_PEOPLE_PERSON_USER_OK_SQL,
  PP_RD24_TIME_ENTRY_REVIEWABLE_GUARD,
  PP_RD24_STATUTORY_PACK_GUARD,
  PP_RD24_APPROVED_RUN_EVIDENCE_GUARD,
  PP_RD24_TARGET_ID_SQL,
];

describe('People Payroll read-tail r24 byte-identical / parameterized family dedupe', () => {
  test('canonical constants own the exact read-only fragments', () => {
    expect(PP_RD24_UNCLAIMED_EXPENSE_GUARD).toBe("NOT EXISTS(SELECT 1 FROM hr_payment_batches WHERE source_type='expense' AND source_id=? AND status!='cancelled')");
    expect(PP_RD24_EXPENSE_AI_JOB_LEASE_GUARD).toBe("EXISTS(SELECT 1 FROM expense_ai_proposals WHERE id=? AND state='running' AND lease_token=?)");
    expect(PP_RD24_PEOPLE_PERSON_USER_SAME_SQL).toBe('SELECT people_person_user($1,$2,$3) same');
    expect(PP_RD24_PEOPLE_PERSON_USER_OK_SQL).toBe('SELECT people_person_user($1,$2,$3) ok');
    expect(PP_RD24_TIME_ENTRY_REVIEWABLE_GUARD).toBe("EXISTS(SELECT 1 FROM hr_time_entries t JOIN hr_engagements e ON e.id=t.engagement_id JOIN hr_people p ON p.id=e.person_id WHERE t.id=? AND t.company_id=? AND t.version=? AND t.status='submitted' AND t.submitted_by!=? AND (p.user_id IS NULL OR p.user_id!=?) AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?))");
    expect(PP_RD24_STATUTORY_PACK_GUARD).toBe("EXISTS(SELECT 1 FROM hr_country_packs WHERE id=? AND status='active' AND record_version=? AND source_digest=?)");
    expect(PP_RD24_APPROVED_RUN_EVIDENCE_GUARD).toBe("EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='approved' AND version=? AND revision=? AND input_digest=?)");
    expect(PP_RD24_TARGET_ID_SQL).toBe('SELECT s.id target_id');
    for (const sql of literals) {
      expect(sql).toContain('SELECT ');
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('each family has one shared symbol and composed callers keep no local copy', () => {
    const families = [
      { name: 'PP_RD24_UNCLAIMED_EXPENSE_GUARD', literal: PP_RD24_UNCLAIMED_EXPENSE_GUARD, callers: [['commerce-expense-payment-effects.ts', 1], ['expenses.ts', 1]] as [string, number][] },
      { name: 'PP_RD24_EXPENSE_AI_JOB_LEASE_GUARD', literal: PP_RD24_EXPENSE_AI_JOB_LEASE_GUARD, callers: [['expense-ai.ts', 2]] as [string, number][] },
      { name: 'PP_RD24_PEOPLE_PERSON_USER_SAME_SQL', literal: PP_RD24_PEOPLE_PERSON_USER_SAME_SQL, callers: [['people-lifecycle.ts', 1], ['technology-provisioning-facts.ts', 1]] as [string, number][] },
      { name: 'PP_RD24_PEOPLE_PERSON_USER_OK_SQL', literal: PP_RD24_PEOPLE_PERSON_USER_OK_SQL, callers: [['people-lifecycle.ts', 1], ['technology-provisioning-facts.ts', 1]] as [string, number][] },
      { name: 'PP_RD24_TIME_ENTRY_REVIEWABLE_GUARD', literal: PP_RD24_TIME_ENTRY_REVIEWABLE_GUARD, callers: [['people-payroll.ts', 2]] as [string, number][] },
      { name: 'PP_RD24_STATUTORY_PACK_GUARD', literal: PP_RD24_STATUTORY_PACK_GUARD, callers: [['people-payroll.ts', 2]] as [string, number][] },
      { name: 'PP_RD24_APPROVED_RUN_EVIDENCE_GUARD', literal: PP_RD24_APPROVED_RUN_EVIDENCE_GUARD, callers: [['people-payroll.ts', 2]] as [string, number][] },
      { name: 'PP_RD24_TARGET_ID_SQL', literal: PP_RD24_TARGET_ID_SQL, callers: [['record-purchases.ts', 2]] as [string, number][] },
    ];
    for (const f of families) {
      for (const [name, count] of f.callers) {
        const s = src(name);
        expect(s, name).not.toContain(quoted(f.literal));
        expect(s.match(new RegExp(f.name, 'g')) ?? [], name).toHaveLength(count + 1);
      }
    }
  });

  test('the shared module declares each fragment exactly once', () => {
    const s = src('pp-read-tail-r24-sources.ts');
    for (const literal of literals) {
      expect(s.split(literal)).toHaveLength(2);
    }
  });

  test('worktree SELECT count is strictly lower than the recorded 1467 r23 baseline', () => {
    const root = new URL('../../../packages/server-people_payroll/src/', import.meta.url);
    const files: URL[] = [];
    const walk = (dir: URL) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = new URL(entry.name + (entry.isDirectory() ? '/' : ''), dir);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) files.push(p);
      }
    };
    walk(root);
    const total = files.reduce((n, p) => n + (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length, 0);
    expect(total).toBeLessThan(1467);
    expect(total).toBe(1195);
  });
});
