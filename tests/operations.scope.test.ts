import assert from 'node:assert/strict';
import test from 'node:test';
import {
  operationsBranchIsAllowed,
  operationsBranchRef,
  operationsHoldingIsAllowed,
  operationsHoldingScopeId,
  operationsPrincipalRefs,
  operationsReadBranchRefs,
  parseOperationsBranchParam,
  type OperationsActorScope,
} from '../lib/operations/scope';

const scope: OperationsActorScope = {
  platformWide: false,
  allowHoldingLevel: false,
  holdingIds: [operationsHoldingScopeId('holding-1')],
  branches: [
    { id: 7, ref: 'branch-ref-7', holdingId: operationsHoldingScopeId('holding-1'), code: 'B7', name: 'Branch 7' },
    { id: 8, ref: 'branch-ref-8', holdingId: operationsHoldingScopeId('holding-1'), code: 'B8', name: 'Branch 8' },
  ],
};

test('operations branch parsing and authorization fail closed', () => {
  assert.deepEqual(parseOperationsBranchParam(undefined), { branchId: null, invalid: false });
  assert.deepEqual(parseOperationsBranchParam('7'), { branchId: 7, invalid: false });
  assert.deepEqual(parseOperationsBranchParam('0'), { branchId: null, invalid: true });
  assert.deepEqual(parseOperationsBranchParam('not-a-branch'), { branchId: null, invalid: true });
  assert.equal(operationsBranchIsAllowed(scope, 7), true);
  assert.equal(operationsBranchIsAllowed(scope, 9), false);
  assert.equal(operationsBranchRef(scope, 7), 'branch-ref-7');
  assert.equal(operationsBranchRef(scope, 9), null);
  assert.deepEqual(operationsReadBranchRefs(scope, 7), ['branch-ref-7']);
  assert.deepEqual(operationsReadBranchRefs(scope, null), ['branch-ref-7', 'branch-ref-8']);
});

test('operations holding scope uses ERPX opaque holding identity', () => {
  assert.equal(operationsHoldingIsAllowed(scope, 'holding-1'), true);
  assert.equal(operationsHoldingIsAllowed(scope, 'holding-2'), false);
  assert.equal(operationsHoldingScopeId('holding-1'), operationsHoldingScopeId('holding-1'));
  assert.notEqual(operationsHoldingScopeId('holding-1'), operationsHoldingScopeId('holding-2'));
});

test('operations principal references are deterministic and actor-derived', () => {
  const refs = operationsPrincipalRefs({ id: 42, employee_code: 'EMP-42', permissions: [] });
  assert.deepEqual(refs, ['folio:user:42', 'folio:employee:EMP-42', '42', 'EMP-42']);
});
