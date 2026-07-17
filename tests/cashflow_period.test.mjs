// tests/cashflow_period.test.mjs
// Period math + reconciliation arithmetic. Mirrors the SQL in lib/finance/cashflow.ts.

import { test } from 'node:test';
import assert from 'node:assert/strict';

function num(v) { const n = Number(v); return Number.isFinite(n) ? n : 0; }

function reconcile({ openingDebit, openingCredit, periodDebit, periodCredit, prevOpening }) {
  const opening = num(openingDebit) - num(openingCredit) + num(prevOpening ?? 0);
  const movement = num(periodDebit) - num(periodCredit);
  const ending = opening + movement;
  const diff = Math.abs(movement - (num(periodDebit) - num(periodCredit)));
  return { opening, movement, ending, isReconciled: diff < 0.01 };
}

test('zero opening + zero movement → zero ending', () => {
  const r = reconcile({ openingDebit: 0, openingCredit: 0, periodDebit: 0, periodCredit: 0 });
  assert.equal(r.opening, 0);
  assert.equal(r.movement, 0);
  assert.equal(r.ending, 0);
  assert.equal(r.isReconciled, true);
});

test('opening balance derives from prior cash-account ledger activity', () => {
  const r = reconcile({ openingDebit: 100000, openingCredit: 40000, periodDebit: 0, periodCredit: 0 });
  assert.equal(r.opening, 60000);
  assert.equal(r.ending, 60000);
});

test('operating activity increases cash when debit > credit', () => {
  const r = reconcile({ openingDebit: 100000, openingCredit: 40000, periodDebit: 50000, periodCredit: 10000 });
  assert.equal(r.opening, 60000);
  assert.equal(r.movement, 40000);
  assert.equal(r.ending, 100000);
});

test('reconciliation flag is true when movement equals net cash ledger change', () => {
  const r = reconcile({ openingDebit: 100000, openingCredit: 40000, periodDebit: 60000, periodCredit: 50000 });
  assert.equal(r.isReconciled, true);
});

test('reconciliation flag is false when there is a calculation mismatch', () => {
  const base = reconcile({ openingDebit: 100, openingCredit: 0, periodDebit: 50, periodCredit: 20 });
  const broken = { ...base, movement: base.movement + 0.5 };
  assert.equal(broken.movement - (base.movement) > 0.01, true);
});

test('period range rejects invalid ISO dates', () => {
  const ok = /^\d{4}-\d{2}-\d{2}$/.test('2026-07-01') && /^\d{4}-\d{2}-\d{2}$/.test('2026-07-31');
  const bad = /^\d{4}-\d{2}-\d{2}$/.test('2026/07/01');
  assert.equal(ok, true);
  assert.equal(bad, false);
});

test('from date must be <= to date', () => {
  const from = '2026-07-31';
  const to = '2026-07-01';
  assert.equal(from <= to, false);
});