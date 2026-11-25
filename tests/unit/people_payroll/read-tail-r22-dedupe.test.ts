import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  PP_RD22_ACTIVE_PERSON_VERSION_GUARD,
  PP_RD22_ENGAGEMENT_JSONB_AGG_SQL,
  PP_RD22_ENGAGEMENT_VERSION_GUARD,
  PP_RD22_LEAVE_REQUESTS_PROJECTION_SQL,
  PP_RD22_WORKFORCE_SCOPE_SQL,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r22-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const quoted = (literal: string) => JSON.stringify(literal);

const literals = [
  PP_RD22_ENGAGEMENT_JSONB_AGG_SQL,
  PP_RD22_LEAVE_REQUESTS_PROJECTION_SQL,
  PP_RD22_WORKFORCE_SCOPE_SQL,
  PP_RD22_ACTIVE_PERSON_VERSION_GUARD,
  PP_RD22_ENGAGEMENT_VERSION_GUARD,
];

describe('People Payroll read-tail r22 byte-identical / parameterized family dedupe', () => {
  test('canonical constants own the exact read-only fragments', () => {
    expect(PP_RD22_ENGAGEMENT_JSONB_AGG_SQL).toBe('SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id) FROM hr_engagements e WHERE e.person_id=p.id AND e.company_id=p.company_id');
    expect(PP_RD22_LEAVE_REQUESTS_PROJECTION_SQL).toBe('SELECT "id","number","employee_id","company_id","branch_id","start_date","days","handover_note","state","version","created_by","prepared_by","decided_by","decision_note","last_command_id","created_at","updated_at","department_id","person_id","engagement_id","leave_type_id","end_date","start_segment","end_segment","policy_version_id","calculated_units","calculation_snapshot","cancellation_state","evidence_policy_snapshot","evidence_due_at","evidence_version","submitted_at" FROM leave_requests');
    expect(PP_RD22_WORKFORCE_SCOPE_SQL).toBe('SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ');
    expect(PP_RD22_ACTIVE_PERSON_VERSION_GUARD).toBe("EXISTS(SELECT 1 FROM hr_people WHERE id=? AND company_id=? AND version=? AND status='active' AND merged_into_id IS NULL)");
    expect(PP_RD22_ENGAGEMENT_VERSION_GUARD).toBe('EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=?)');
    for (const sql of literals) {
      expect(sql).toContain('SELECT ');
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('each family has one shared symbol and composed callers keep no local copy', () => {
    const families = [
      { name: 'PP_RD22_ENGAGEMENT_JSONB_AGG_SQL', literal: PP_RD22_ENGAGEMENT_JSONB_AGG_SQL, callers: [
        ['learning-worker-references.ts', 1], ['project-worker-references.ts', 1],
      ] as [string, number][] },
      { name: 'PP_RD22_LEAVE_REQUESTS_PROJECTION_SQL', literal: PP_RD22_LEAVE_REQUESTS_PROJECTION_SQL, callers: [
        ['last-audit-owner-projection-sources.ts', 1], ['platform-owner-projection-sources.ts', 1],
      ] as [string, number][] },
      { name: 'PP_RD22_WORKFORCE_SCOPE_SQL', literal: PP_RD22_WORKFORCE_SCOPE_SQL, callers: [
        ['attachment-read.ts', 1], ['self-service-product-entry.ts', 1],
      ] as [string, number][] },
      { name: 'PP_RD22_ACTIVE_PERSON_VERSION_GUARD', literal: PP_RD22_ACTIVE_PERSON_VERSION_GUARD, callers: [
        ['payroll-adjustments.ts', 1], ['people-handoffs.ts', 1],
      ] as [string, number][] },
      { name: 'PP_RD22_ENGAGEMENT_VERSION_GUARD', literal: PP_RD22_ENGAGEMENT_VERSION_GUARD, callers: [
        ['people-evaluation.ts', 1], ['operations-staff-settings-facts.ts', 1],
      ] as [string, number][] },
    ];
    for (const f of families) {
      for (const [name, count] of f.callers) {
        const s = src(name);
        expect(s, name).not.toContain(quoted(f.literal));
        expect(s.match(new RegExp(f.name, 'g')) ?? [], name).toHaveLength(count + 1);
      }
    }
    expect(src('learning-worker-references.ts')).toContain('(${PP_RD22_ENGAGEMENT_JSONB_AGG_SQL})) ORDER BY p.id');
    expect(src('last-audit-owner-projection-sources.ts')).toContain('leave_requests: PP_RD22_LEAVE_REQUESTS_PROJECTION_SQL');
    expect(src('attachment-read.ts')).toContain('`${PP_RD22_WORKFORCE_SCOPE_SQL}${current.where}`');
    expect(src('self-service-product-entry.ts')).toContain('`${PP_RD22_WORKFORCE_SCOPE_SQL}(${current.where}) AND EXISTS');
    expect(src('payroll-adjustments.ts')).toContain('PP_RD22_ACTIVE_PERSON_VERSION_GUARD');
    expect(src('people-evaluation.ts')).toContain('PP_RD22_ENGAGEMENT_VERSION_GUARD');
  });

  test('the shared module declares each fragment exactly once', () => {
    const s = src('pp-read-tail-r22-sources.ts');
    for (const literal of literals) {
      expect(s.split(literal)).toHaveLength(2);
    }
  });

  test('worktree read-only SELECT count is strictly lower than the recorded 1491 r21 baseline', () => {
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
    expect(total).toBeLessThan(1491);
    expect(total).toBe(1202);
  });
});
