import test from 'node:test';
import assert from 'node:assert/strict';
import { matchPerm, parseRoleId } from '../lib/perm/grammar';
import {
  evaluateExpenseStageRule,
  nextExpenseStage,
  requiresExecutiveApproval,
  type ExpenseRuleActor,
  type ExpenseRuleContext,
} from '../lib/waybill/expenseRules';

const base: ExpenseRuleContext = {
  stage: 'department_approval',
  amount: 1_000,
  submitterId: 10,
  submitterDepartmentId: 'development',
  departmentHeadId: null,
  departmentTopRank: 5,
  accountingHeadId: 30,
  accountingTopRank: 4,
  accrualPreparerId: 31,
};

function actor(patch: Partial<ExpenseRuleActor> = {}): ExpenseRuleActor {
  return {
    id: 20,
    departmentId: 'development',
    rank: 5,
    roleName: 'supervisor',
    ...patch,
  };
}

test('stable roles expose explicit canonical ranks', () => {
  assert.deepEqual(parseRoleId('manager'), { name: 'manager', level: 4 });
  assert.deepEqual(parseRoleId('ceo'), { name: 'ceo', level: 1 });
});

test('deny permission takes precedence over an allow', () => {
  assert.equal(matchPerm([
    'finance:expense:pay::allow',
    'finance:expense:pay::deny',
  ], 'finance:expense:pay::allow'), false);
});

test('supervisor approves only when no higher active department member exists', () => {
  assert.equal(evaluateExpenseStageRule(actor(), base).allow, true);
  assert.equal(evaluateExpenseStageRule(actor(), { ...base, departmentTopRank: 4 }).allow, false);
  assert.equal(evaluateExpenseStageRule(actor({ rank: 4, roleName: 'manager' }), {
    ...base,
    departmentTopRank: 4,
  }).allow, true);
});

test('only a designated department head may self-approve', () => {
  assert.equal(evaluateExpenseStageRule(actor({ id: 10 }), base).allow, false);
  assert.equal(evaluateExpenseStageRule(actor({ id: 10 }), {
    ...base,
    departmentHeadId: 10,
  }).allow, true);
});

test('accounting approver differs from submitter and preparer', () => {
  const ctx = { ...base, stage: 'accounting_approval' as const };
  assert.equal(evaluateExpenseStageRule(actor({
    id: 30,
    departmentId: 'accounting',
    rank: 4,
    roleName: 'manager',
  }), ctx).allow, true);
  assert.equal(evaluateExpenseStageRule(actor({
    id: 31,
    departmentId: 'accounting',
    rank: 4,
    roleName: 'manager',
  }), { ...ctx, accountingHeadId: 31 }).allow, false);
});

test('THB 200,000 skips executive approval while THB 200,000.01 requires it', () => {
  assert.equal(requiresExecutiveApproval(200_000), false);
  assert.equal(nextExpenseStage('accounting_approval', 200_000), 'payment');
  assert.equal(requiresExecutiveApproval(200_000.01), true);
  assert.equal(nextExpenseStage('accounting_approval', 200_000.01), 'executive_approval');
});

test('either CFO or CEO may approve high value except their own claim', () => {
  const ctx = { ...base, stage: 'executive_approval' as const, amount: 200_000.01 };
  assert.equal(evaluateExpenseStageRule(actor({
    id: 40,
    departmentId: 'executive',
    rank: 2,
    roleName: 'cfo',
  }), ctx).allow, true);
  assert.equal(evaluateExpenseStageRule(actor({
    id: 10,
    departmentId: 'executive',
    rank: 1,
    roleName: 'ceo',
  }), ctx).allow, false);
});

test('payment is restricted to Finance and settlement to Accounting', () => {
  assert.equal(evaluateExpenseStageRule(actor({ departmentId: 'finance' }), {
    ...base,
    stage: 'payment',
  }).allow, true);
  assert.equal(evaluateExpenseStageRule(actor({ departmentId: 'accounting' }), {
    ...base,
    stage: 'payment',
  }).allow, false);
  assert.equal(evaluateExpenseStageRule(actor({ departmentId: 'accounting' }), {
    ...base,
    stage: 'settlement',
  }).allow, true);
});
