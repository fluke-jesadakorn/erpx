import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { projectIdSql } from '../../../packages/server-projects/src/adapters/project-id-source';
import { projectPointReadHead } from '../../../packages/server-projects/src/adapters/read-tail-r33';

const srcRoot = resolve(__dirname, '../../../packages/server-projects/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Verbatim pre-r35 inline point read from HEAD; byte-identity oracle only, never a live query source.
const preEdit = (table: string) => `SELECT project_id FROM ${table} WHERE id=? AND company_id=?`;

describe('Projects read-tail r35 point-read family merge', () => {
  test('projectIdSql and the r33 point-read head emit the same SQL byte for byte', () => {
    for (const table of ['staff_tasks', 'staff_project_handoffs']) {
      expect(projectIdSql(table)).toBe(preEdit(table));
      expect(projectPointReadHead('project_id', table, 'company_id')).toBe(preEdit(table));
      expect(projectIdSql(table)).toBe(projectPointReadHead('project_id', table, 'company_id'));
      expect(preEdit(table).match(/SELECT /g)).toHaveLength(1);
    }
  });

  test('the standalone literal is gone and both call sites compose the shared head', () => {
    const id = src('project-id-source.ts');
    expect(id).toContain("import {projectPointReadHead} from './read-tail-r33';");
    expect(id).toContain("projectPointReadHead('project_id',table,'company_id')");
    expect(id).toContain('projectIdSql');
    expect(id).not.toContain('`SELECT project_id FROM ${table} WHERE id=? AND company_id=?`');

    const entry = src('product-entry.ts');
    expect(entry).toContain("import {projectIdSql} from './project-id-source';");
    expect(entry).toContain("projectIdSql('staff_tasks')");
    expect(entry).toContain('.bind(target.id,actor.companyId)');

    const structure = src('project-structure-sources.ts');
    expect(structure).not.toContain("projectIdSql('staff_project_handoffs')");
    // Lane 27 wave 2: the handoff reads are native; the TS adapter only transports.
    expect(structure).toContain('nativeProjectHandoffProject');
    expect(structure).toContain('nativeProjectHandoffWork');
    expect(structure).toContain('nativeProjectHandoffs');
  });

  test('locks, spaced JOINs and every r2-r34 helper stay untouched', () => {
    for (const [file, symbol] of [
      ['read-tail-r23.ts', 'projectAssignmentAuthorityReadSql'],
      ['read-tail-r27.ts', 'projectMemberExistsSql'],
      ['read-tail-r28.ts', 'assignmentViewerSelect'],
      ['read-tail-r29.ts', 'orderedImageAggregateHead'],
      ['read-tail-r32.ts', 'projectPurchaseSourceHead'],
      ['read-tail-r33.ts', 'projectPointReadHead'],
      ['read-tail-r34.ts', 'staffTaskPointHead'],
      ['clock-cte.ts', 'export const projectsClockCte='],
    ] as const) expect(src(file)).toContain(symbol);
    expect(src('package-obligations.ts')).toContain('LOCK TABLE staff_projects,staff_project_members,staff_project_milestones');
    expect(src('purchases.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    expect(src('social-purchase-sources.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE MODE');
    expect(src('people-evaluation-facts.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    expect(src('scheduled-tasks.ts')).toContain('FOR UPDATE OF t');
    expect(src('scheduled-tasks.ts')).toContain('FOR SHARE');
    expect(src('task-purchases.ts')).toContain("${lock?' FOR UPDATE':''}");
    expect(src('executive-evidence-chains.ts')).toContain('JOIN (${sourceSql}) p ON p.id=m.project_id AND p.company_id=m.company_id');
  });

  test('retirement:ts-statements is 74 after r40 (BEFORE=81)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r34 left 96. r35 removes the standalone projectIdSql literal (1->0) by
    // composing the r33 point-read head; its two call sites emit unchanged SQL.
    expect(total).toBe(65); // r44 final-root projection family folded into read-tail-r40 (66->65)
    expect(total).toBeLessThan(88);
  });
});
