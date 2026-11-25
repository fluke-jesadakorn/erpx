import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {assignmentReadSql} from '@erpx/server-identity_access/public/record-source-authority';
import {actorScopeProbe,currentAssignmentScopeHead,moduleGrantAssignmentProbe} from '../../../packages/server-learning/src/adapters/learning-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-learning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-r9 literals copied verbatim from HEAD. Byte-identity oracles only, never live sources.
// The interpolations are the same external/owner calls the original callers used.
const permission = 'records.read';
const preMetric = `SELECT ${assignmentReadSql('learning_metric_scope',permission)} allowed FROM ${actorScopeProbe}) learning_metric_scope`;
const purchaseGrant = `\n AND ${moduleGrantAssignmentProbe('learning_module',`learning_module.id=? AND learning_module.company_id=learning_scope.company_id\n AND learning_module.user_id=? AND learning_module.role=?`)}`;
const prePurchase = `SELECT ${assignmentReadSql('learning_scope',permission)}${purchaseGrant} allowed FROM\n ${actorScopeProbe}) learning_scope`;

describe('Learning read-tail r9 current assignment read-scope head', () => {
  test('renders both pre-dedupe scope heads byte-for-byte', () => {
    expect(currentAssignmentScopeHead('learning_metric_scope',permission)).toBe(preMetric);
    expect(currentAssignmentScopeHead('learning_scope',permission,purchaseGrant,'\n ')).toBe(prePurchase);
    readOnly(preMetric);
    readOnly(prePurchase);
  });

  test('family is defined once and composed at both callers', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const currentAssignmentScopeHead =/g) ?? []).toHaveLength(1);
    const purchases = src('purchases.ts');
    expect(purchases.match(/currentAssignmentScopeHead\(/g) ?? []).toHaveLength(1);
    expect(purchases).not.toContain('allowed FROM\n ${actorScopeProbe}) learning_scope');
    const metric = src('metric-purchases.ts');
    expect(metric.match(/currentAssignmentScopeHead\(/g) ?? []).toHaveLength(1);
    expect(metric).not.toContain('allowed FROM ${actorScopeProbe}) learning_metric_scope');
  });

  test('r1-r8 helpers and all 7 owner LOCK TABLE statements stay untouched', () => {
    const shared = src('learning-read-sql.ts');
    for (const name of ['actorScopeProbe', 'clockProbe', 'coursesImage', 'learningNativeSources', 'nativeSourceProbe', 'overdueEvidenceColumns', 'learningModuleGrantProbe', 'moduleGrantAssignmentProbe', 'actorScopeExistsProbe', 'assignmentsImage', 'sourceLiteral', 'metricLabelProjection', 'learningAssignmentsExistsProbe']) {
      expect(shared.match(new RegExp(`export const ${name} ?=`, 'g')) ?? []).toHaveLength(1);
    }
    expect(src('native_lane_31/postgres.rs')).toContain('SELECT e.id,e.assignment_id,e.version event_version,e.status,e.created_at,c.title course_title');
    expect(src('learning.ts').match(/const assignmentChildImage =/g) ?? []).toHaveLength(1);
    expect(src('executive-metric-projections.ts').match(/const orgNodeLabel =/g) ?? []).toHaveLength(1);
    let locks = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) locks += (readFileSync(path, 'utf8').match(/LOCK TABLE/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(locks).toBe(7);
  });

  test('retirement:ts-statements is strictly lower than the r9 BEFORE=70 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(70);
    expect(total).toBeLessThanOrEqual(69);
  });
});
