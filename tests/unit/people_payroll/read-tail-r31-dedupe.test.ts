import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { ppRd31StatusGuard } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r31-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol, 'g')) ?? []).length;

describe('People Payroll read-tail r31 parameterized family dedupe', () => {
  test('canonical builder owns the exact read-only status guards', () => {
    const exists = ppRd31StatusGuard(false, 'hr_compensation_revisions', 'id', 'draft');
    expect(exists).toBe("EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE id=? AND status='draft')");
    readOnly(exists);
    const notExists = ppRd31StatusGuard(true, 'hr_evaluation_compensation_drafts', 'assessment_id', 'draft');
    expect(notExists).toBe("NOT EXISTS(SELECT 1 FROM hr_evaluation_compensation_drafts WHERE assessment_id=? AND status='draft')");
    readOnly(notExists);
  });

  test('each caller keeps no local copy and uses the shared builder', () => {
    const callers: Array<[string, string, string]> = [
      ['people-evaluation.ts', "NOT EXISTS(SELECT 1 FROM hr_evaluation_compensation_drafts WHERE assessment_id=? AND status='draft')", "ppRd31StatusGuard(true, 'hr_evaluation_compensation_drafts', 'assessment_id', 'draft')"],
      ['people-evaluation.ts', "EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE id=? AND status='draft')", "ppRd31StatusGuard(false, 'hr_compensation_revisions', 'id', 'draft')"],
      ['people-payroll.ts', "NOT EXISTS(SELECT 1 FROM hr_payroll_advances WHERE engagement_id=? AND status='pending')", "ppRd31StatusGuard(true, 'hr_payroll_advances', 'engagement_id', 'pending')"],
      ['people-payroll.ts', "EXISTS(SELECT 1 FROM hr_payment_batch_items WHERE batch_id=? AND status='failed')", "ppRd31StatusGuard(false, 'hr_payment_batch_items', 'batch_id', 'failed')"],
    ];
    for (const [name, literal, call] of callers) {
      expect(src(name), name).not.toContain(literal);
      expect(src(name), name).toContain(call);
    }
    expect(uses('people-evaluation.ts', 'ppRd31StatusGuard\\(')).toBe(2);
    expect(uses('people-payroll.ts', 'ppRd31StatusGuard\\(')).toBe(2);
  });

  test('the shared module declares the read fragment exactly once', () => {
    const s = src('pp-read-tail-r31-sources.ts');
    expect((s.match(/export const /g) ?? []).length).toBe(1);
    expect(s.includes("EXISTS(SELECT 1 FROM ${table} WHERE ${col}=? AND status='${status}')")).toBe(true);
  });

  test('lock-bearing and cross-owner reads in touched owners remain untouched', () => {
    expect(src('people-evaluation.ts')).toContain('SELECT id FROM hr_engagements WHERE id=? FOR UPDATE');
    expect(src('people-evaluation.ts')).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(src('people-payroll.ts')).toContain('organizationFinalSources.people_payroll_position_pay_defaults_from');
  });

  test('worktree SELECT count is strictly lower than the recorded 1412 r30 baseline', () => {
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
    expect(total).toBeLessThan(1412);
    expect(total).toBe(1356);
  });
});
