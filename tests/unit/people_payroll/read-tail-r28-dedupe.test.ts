import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  ppRd28ApprovalChainSql,
  ppRd28LifecycleCasesProjectionSql,
  PP_RD28_ADVANCE_REPAYMENTS_SQL,
  PP_RD28_CONTRACTOR_COUNT_SQL,
  PP_RD28_PENDING_TIME_COUNT_SQL,
  ppRd28PersonLegalNamesSql,
  ppRd28SourceExistsByIdSql,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r28-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol, 'g')) ?? []).length;

describe('People Payroll read-tail r28 parameterized family dedupe', () => {
  test('canonical builders own the exact read-only fragments', () => {
    const chain = ppRd28ApprovalChainSql('LEFT JOIN hr_levels l');
    expect(chain).toContain('EXISTS(WITH RECURSIVE chain AS (');
    expect(chain).toContain('LEFT JOIN hr_levels l ON l.id=e.level_id');
    expect(chain.match(/SELECT /g) ?? []).toHaveLength(3);

    const lifecycle = ppRd28LifecycleCasesProjectionSql('MARKED_ROWS', 'SOURCE_ROWS');
    expect(lifecycle).toBe('SELECT c.id,c.version,c.status,to_jsonb(c)::text image,source.proof FROM people_lifecycle_cases c JOIN (MARKED_ROWS) marked ON marked.id=c.id AND marked.company_id=c.company_id LEFT JOIN (SOURCE_ROWS) source ON source.id=c.id AND source.company_id=c.company_id WHERE c.company_id=? ORDER BY c.id');

    const names = ppRd28PersonLegalNamesSql('?,?');
    expect(names).toBe('SELECT user_id,legal_name FROM hr_people WHERE company_id=? AND merged_into_id IS NULL AND user_id IN (?,?)');

    const exists = ppRd28SourceExistsByIdSql('hr_people', 'workspace_records.id', '');
    expect(exists).toBe('EXISTS(SELECT 1 FROM hr_people s WHERE s.id=workspace_records.id)');
    const existsExtra = ppRd28SourceExistsByIdSql('hr_people', 'payroll_input.id', " AND s.clearance_type IS NOT NULL");
    expect(existsExtra).toBe('EXISTS(SELECT 1 FROM hr_people s WHERE s.id=payroll_input.id AND s.clearance_type IS NOT NULL)');

    const constants: string[] = [
      PP_RD28_ADVANCE_REPAYMENTS_SQL,
      PP_RD28_CONTRACTOR_COUNT_SQL,
      PP_RD28_PENDING_TIME_COUNT_SQL,
    ];
    expect(constants[0]).toContain('FROM hr_payroll_advance_repayments r JOIN hr_payroll_advances a ON a.id=r.advance_id');
    expect(constants[1]).toContain("e.worker_type='contractor' AND e.status IN ('active','ended')");
    expect(constants[2]).toContain("t.status IN ('draft','submitted')");
    for (const literal of [chain, lifecycle, names, exists, existsExtra, ...constants]) readOnly(literal);
  });

  test('each caller keeps no local copy and uses the shared symbol', () => {
    expect(src('workflow-approvals-facts.ts')).not.toContain('EXISTS(WITH RECURSIVE chain AS (');
    expect(uses('workflow-approvals-facts.ts', 'ppRd28ApprovalChainSql\\(')).toBe(2);

    for (const name of ['project-package-obligations.ts', 'learning-package-obligations.ts']) {
      expect(src(name), name).not.toContain('to_jsonb(c)::text image,source.proof');
      expect(uses(name, 'ppRd28LifecycleCasesProjectionSql\\('), name).toBe(1);
    }

    for (const name of ['people-payroll.ts', 'payroll-readiness.ts']) {
      expect(src(name), name).not.toContain('SELECT r.id,r.version,r.amount_minor,r.due_period_start,r.due_period_end,a.reason FROM hr_payroll_advance_repayments');
      expect(uses(name, 'PP_RD28_ADVANCE_REPAYMENTS_SQL'), name).toBeGreaterThanOrEqual(1);
    }
    expect(src('payroll-readiness.ts')).not.toContain("e.worker_type='contractor' AND e.status IN ('active','ended') AND p.status='active' AND e.start_date<=?");
    expect(src('payroll-readiness.ts')).not.toContain('COUNT(*) AS count FROM hr_time_entries t JOIN hr_engagements e');
    expect(uses('payroll-readiness.ts', 'PP_RD28_CONTRACTOR_COUNT_SQL')).toBeGreaterThanOrEqual(2);
    expect(uses('payroll-readiness.ts', 'PP_RD28_PENDING_TIME_COUNT_SQL')).toBeGreaterThanOrEqual(2);

    for (const name of ['finance-group-person-facts.ts', 'records-request-preview-facts.ts']) {
      expect(src(name), name).not.toContain('user_id,legal_name FROM hr_people WHERE company_id=? AND merged_into_id');
      expect(uses(name, 'ppRd28PersonLegalNamesSql\\('), name).toBe(1);
    }

    expect(src('record-purchases.ts')).not.toContain('EXISTS(SELECT 1 FROM ${tables[kind]} s WHERE s.id=${id}');
    expect(src('record-purchases.ts')).not.toContain('EXISTS(SELECT 1 FROM ${peopleSourceTables[kind]} s WHERE s.id=${id}');
    expect(uses('record-purchases.ts', 'ppRd28SourceExistsByIdSql\\(')).toBe(2);
  });

  test('the shared module declares each fragment exactly once', () => {
    const s = src('pp-read-tail-r28-sources.ts');
    expect((s.match(/export const /g) ?? []).length).toBe(7);
    for (const fragment of [
      'EXISTS(WITH RECURSIVE chain AS (',
      'SELECT c.id,c.version,c.status,to_jsonb(c)::text image,source.proof FROM people_lifecycle_cases c JOIN (${markedRows})',
      'FROM hr_payroll_advance_repayments r JOIN hr_payroll_advances a ON a.id=r.advance_id',
      "e.worker_type='contractor' AND e.status IN ('active','ended')",
      "t.status IN ('draft','submitted')",
      'SELECT user_id,legal_name FROM hr_people WHERE company_id=? AND merged_into_id IS NULL AND user_id IN (${inList})',
      'EXISTS(SELECT 1 FROM ${table} s WHERE s.id=${idSql}${extra})',
    ]) {
      expect(s.includes(fragment), fragment).toBe(true);
    }
  });

  test('lock-bearing reads in touched owners remain untouched', () => {
    expect(src('payroll-readiness.ts')).toContain('FOR UPDATE OF r SKIP LOCKED');
    expect(src('payroll-readiness.ts')).toContain('FOR SHARE');
    expect(src('project-package-obligations.ts')).toContain('LOCK TABLE hr_people,hr_engagements,people_lifecycle_cases');
    expect(src('people-evaluation.ts')).toContain('SELECT id FROM hr_engagements WHERE id=? FOR UPDATE');
    expect(src('people-evaluation.ts')).toContain('SELECT id FROM hr_evaluation_definitions WHERE id=? FOR SHARE');
  });

  test('worktree SELECT count is strictly lower than the recorded 1432 r27 baseline', () => {
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
    expect(total).toBeLessThan(1432);
    expect(total).toBe(1210);
  });
});
