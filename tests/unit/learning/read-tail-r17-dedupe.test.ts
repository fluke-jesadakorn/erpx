import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { clockedSourceCte, existsRowProbe, keyedRowRead, scopeRow } from '../../../packages/server-learning/src/adapters/learning-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-learning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-r17 literals copied verbatim from HEAD. Byte-identity oracles only, never live sources.
const preSensitiveVersion = 'EXISTS(SELECT 1 FROM learning_courses WHERE id=? AND company_id=? AND version=?)';
const preFileSource = `EXISTS(SELECT 1 FROM (COURSE_SOURCE) learning_file_source WHERE image=? AND TRUE AND iam_role_allows(?,?,'learning.documents.manage'))`;
const preActorScope = 'SELECT 1 FROM(ACTOR_SCOPE)workforce_actor_scope WHERE a.branch_id=?';
const preClockEval = `EXISTS(${clockedSourceCte('evaluation_learning_clock', '')} SELECT 1 FROM evaluation_learning_clock WHERE CAPABILITY_PREDICATE)`;

const preReceipt = `SELECT operation FROM command_receipts WHERE command_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?`;
const preMetric = `${clockedSourceCte('learning_metric_clock')} SELECT learning_metric_owned.* FROM learning_assignments learning_metric_owned CROSS JOIN learning_metric_clock WHERE METRIC_PREDICATE`;
const preRecordScope = `SELECT EXISTS(SCOPE) allowed FROM ${scopeRow('?::text id,?::text company_id')} learning_record`;
const preSettlement = `SELECT a.*,c.company_id course_company_id,c.required_for
 FROM learning_assignments a LEFT JOIN learning_courses c ON c.id=a.course_id
 LEFT JOIN command_receipts command ON event.id='learning-assignment-event-'||command.command_id`;

describe('Learning read-tail r17 existsRowProbe', () => {
  test('all four existence probes render byte-for-byte with their original frames', () => {
    expect(`EXISTS(${existsRowProbe('FROM learning_courses', 'id=? AND company_id=? AND version=?')})`).toBe(preSensitiveVersion);
    expect(`EXISTS(${existsRowProbe('FROM (COURSE_SOURCE) learning_file_source', `image=? AND TRUE AND iam_role_allows(?,?,'learning.documents.manage')`)})`).toBe(preFileSource);
    expect(existsRowProbe('FROM(ACTOR_SCOPE)workforce_actor_scope', 'a.branch_id=?')).toBe(preActorScope);
    expect(`EXISTS(${clockedSourceCte('evaluation_learning_clock', '')} ${existsRowProbe('FROM evaluation_learning_clock', 'CAPABILITY_PREDICATE')})`).toBe(preClockEval);
    for (const sql of [preSensitiveVersion, preFileSource, preActorScope, preClockEval]) readOnly(sql);
  });

  test('family is defined once and composed at four callers with binds unchanged', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const existsRowProbe =/g) ?? []).toHaveLength(1);
    for (const file of ['attachments-sensitive-facts.ts', 'attachment-purchases.ts', 'attachment-read.ts', 'evaluation-source-policy.ts']) {
      expect(src(file).match(/existsRowProbe\(/g) ?? []).toHaveLength(1);
    }
    expect(src('attachments-sensitive-facts.ts')).not.toContain(preSensitiveVersion);
    expect(src('attachment-purchases.ts')).not.toContain(preFileSource);
    expect(src('attachment-read.ts')).not.toContain(preActorScope);
    expect(src('evaluation-source-policy.ts')).not.toContain(preClockEval);
    expect(src('attachment-read.ts')).toContain('.bind(actor.companyId,actor.branchId,actor.departmentId,...currentScope.args)');
  });
});

describe('Learning read-tail r17 keyedRowRead', () => {
  test('all four keyed reads render byte-for-byte with their original frames', () => {
    expect(keyedRowRead('operation', ' FROM command_receipts', ` WHERE command_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?`)).toBe(preReceipt);
    expect(`${clockedSourceCte('learning_metric_clock')} ${keyedRowRead('learning_metric_owned.*', ' FROM learning_assignments learning_metric_owned CROSS JOIN learning_metric_clock', ' WHERE METRIC_PREDICATE')}`).toBe(preMetric);
    expect(keyedRowRead('EXISTS(SCOPE) allowed', ` FROM ${scopeRow('?::text id,?::text company_id')} learning_record`)).toBe(preRecordScope);
    expect(keyedRowRead('a.*,c.company_id course_company_id,c.required_for', `
 FROM learning_assignments a LEFT JOIN learning_courses c ON c.id=a.course_id
 LEFT JOIN command_receipts command ON event.id='learning-assignment-event-'||command.command_id`)).toBe(preSettlement);
    for (const sql of [preReceipt, preMetric, preRecordScope, preSettlement]) readOnly(sql);
  });

  test('family is defined once and composed at four callers with binds unchanged', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const keyedRowRead =/g) ?? []).toHaveLength(1);
    for (const file of ['metric-purchases.ts', 'purchases.ts', 'record-purchases.ts', 'package-obligations.ts']) {
      expect(src(file).match(/keyedRowRead\(/g) ?? []).toHaveLength(1);
    }
    expect(src('purchases.ts')).not.toContain(preReceipt);
    expect(src('metric-purchases.ts')).not.toContain(preMetric);
    expect(src('record-purchases.ts')).not.toContain(preRecordScope);
    expect(src('package-obligations.ts')).not.toContain(preSettlement);
    expect(src('purchases.ts')).toContain('.bind(command.key,actor.userId,actor.companyId)');
  });
});

describe('Learning read-tail r17 invariants', () => {
  test('r1-r16 helpers, native sources and all 7 owner LOCK TABLE statements stay untouched', () => {
    const shared = src('learning-read-sql.ts');
    for (const name of [
      'actorScopeProbe', 'clockProbe', 'coursesImage', 'learningNativeSources', 'nativeSourceProbe',
      'overdueEvidenceColumns', 'learningModuleGrantProbe', 'moduleGrantAssignmentProbe', 'actorScopeExistsProbe',
      'assignmentsImage', 'sourceLiteral', 'metricLabelProjection', 'learningAssignmentsExistsProbe',
      'currentAssignmentScopeHead', 'learningCourseProjection', 'assignmentCourseJoinHead',
      'assignmentCourseReadHead', 'packageScopeCte', 'benefitConsumptionHead', 'latestEventLateral',
      'identitySourceRead', 'workspaceImageFrame', 'clockedSourceCte', 'obligationFactHead',
      'imageChildFrame', 'certificateConsistencyProbe', 'jsonbAggImageHead', 'scopeRow',
    ]) {
      expect(shared.match(new RegExp(`export const ${name} ?=`, 'g')) ?? []).toHaveLength(1);
    }
    const native = shared.match(/export const learningNativeSources=`([\s\S]*?)`;/)?.[1];
    expect(native).toBeDefined();
    expect((native!.match(/SELECT /g) ?? []).length).toBe(7);
    expect((native!.match(/UNION ALL SELECT /g) ?? []).length).toBe(6);
    expect(src('learning-completion-evidence.ts').match(/export const completionEvidenceColumns =/g) ?? []).toHaveLength(1);
    expect(src('learning.ts').match(/const assignmentChildImage =/g) ?? []).toHaveLength(1);
    expect(src('learning.ts').match(/const learnerEligible =/g) ?? []).toHaveLength(1);
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

  test('retirement:ts-statements is strictly lower than the r17 BEFORE=45 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(45);
    // r18 lowers this further; r17 only pins that its own baseline decreased.
    expect(total).toBeLessThanOrEqual(38);
  });
});
