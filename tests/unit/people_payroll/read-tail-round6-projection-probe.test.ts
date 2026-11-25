import { describe, expect, test } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { ppRdRound6ProjectionByTwoPred } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-round6-sources';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('People Payroll round-6 projection-over-source probe family', () => {
  test('one canonical builder renders both former templates byte-identically', () => {
    const retained = ppRdRound6ProjectionByTwoPred('account_id', 'SOURCE', 'source', 'person_id', 'company_id');
    const terminal = ppRdRound6ProjectionByTwoPred('1', 'SOURCE', 'terminal', 'id', 'company_id');
    expect(retained).toBe('SELECT account_id FROM (SOURCE) source WHERE source.person_id=? AND source.company_id=?');
    expect(terminal).toBe('SELECT 1 FROM (SOURCE) terminal WHERE terminal.id=? AND terminal.company_id=?');
    for (const sql of [retained, terminal]) {
      expect((sql.match(/\?/g) ?? []).length).toBe(2);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('both callers delegate and keep no inline copy', () => {
    expect(src('project-worker-startup-state.ts')).toContain("from './pp-read-tail-round6-sources'");
    expect(src('project-worker-startup-state.ts')).toContain("ppRdRound6ProjectionByTwoPred('account_id', projectWorkerSourceRows, 'source', 'person_id', 'company_id')");
    expect(src('project-worker-startup-state.ts')).not.toContain('SELECT account_id FROM (${projectWorkerSourceRows}) source WHERE source.person_id=? AND source.company_id=?');
    expect(src('project-worker-startup-jobs.ts')).toContain("from './pp-read-tail-round6-sources'");
    expect(src('project-worker-startup-jobs.ts')).toContain("ppRdRound6ProjectionByTwoPred('1', projectWorkerPlacementTerminalRows, 'terminal', 'id', 'company_id')");
    expect(src('project-worker-startup-jobs.ts')).not.toContain('SELECT 1 FROM (${projectWorkerPlacementTerminalRows}) terminal WHERE terminal.id=? AND terminal.company_id=?');
    const canonical = src('pp-read-tail-round6-sources.ts');
    expect((canonical.match(/export const /g) ?? []).length).toBe(1);
    expect((canonical.match(/SELECT /g) ?? []).length).toBe(1);
    expect(canonical).not.toContain('@erpx/server-');
  });

  test('the fold lowers the owner SELECT count below 1141', () => {
    const root = new URL('../../../packages/server-people_payroll/src/', import.meta.url);
    const files: URL[] = [];
    const walk = (dir: URL) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = new URL(entry.name + (entry.isDirectory() ? '/' : ''), dir);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) files.push(path);
      }
    };
    walk(root);
    const total = files.reduce((n, path) => n + (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length, 0);
    expect(total).toBeLessThan(1141);
    expect(total).toBeLessThanOrEqual(1140);
  });
});
