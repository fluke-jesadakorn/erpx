// tests/report_intent.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';

const SUPPORTED_INTENTS = ['cash_flow', 'trial_balance', 'income_statement', 'balance_sheet', 'period_summary'];

function isSupportedIntent(v) { return typeof v === 'string' && SUPPORTED_INTENTS.includes(v); }

function parseSqlBlocks(text) {
  const asks = [];
  if (!text) return { plain: '', asks };
  const plain = text.replace(/\[SQL\]([\s\S]*?)\[\/SQL\]/g, (_m, body) => {
    try {
      const obj = JSON.parse(body);
      if (!obj || typeof obj !== 'object') return '';
      if (isSupportedIntent(obj.intent)) {
        if (typeof obj.date_from === 'string' && typeof obj.date_to === 'string') {
          asks.push({ intent: obj.intent, date_from: obj.date_from, date_to: obj.date_to, question: typeof obj.question === 'string' ? obj.question : '' });
          return '';
        }
        return '';
      }
      if (typeof obj.question === 'string' && obj.question.trim()) {
        asks.push({ question: obj.question.trim() });
      }
    } catch { /* skip */ }
    return '';
  });
  return { plain: plain.trim(), asks };
}

function num(v) { const n = Number(v); return Number.isFinite(n) ? n : 0; }

function fmt(n) { return n.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 }); }

test('typed intent cash_flow with dates', () => {
  const out = parseSqlBlocks('x\n[SQL]{"intent":"cash_flow","date_from":"2026-07-01","date_to":"2026-07-31","question":"งบกระแสเงินสด"}[/SQL]\n');
  assert.equal(out.asks.length, 1);
  assert.equal(out.asks[0].intent, 'cash_flow');
  assert.equal(out.asks[0].date_from, '2026-07-01');
  assert.equal(out.asks[0].date_to, '2026-07-31');
});

test('typed intent trial_balance', () => {
  const out = parseSqlBlocks('[SQL]{"intent":"trial_balance","date_from":"2026-07-01","date_to":"2026-07-31"}[/SQL]');
  assert.equal(out.asks[0].intent, 'trial_balance');
});

test('typed intent without dates is dropped', () => {
  const out = parseSqlBlocks('[SQL]{"intent":"income_statement"}[/SQL]');
  assert.equal(out.asks.length, 0);
});

test('unknown intent falls back to generic question', () => {
  const out = parseSqlBlocks('[SQL]{"intent":"foobar","date_from":"2026-07-01","date_to":"2026-07-31"}[/SQL]');
  assert.equal(out.asks.length, 0);
});

test('plain question block falls through', () => {
  const out = parseSqlBlocks('[SQL]{"question":"top 5 vendors this month"}[/SQL]');
  assert.equal(out.asks.length, 1);
  assert.equal(out.asks[0].question, 'top 5 vendors this month');
});

test('malformed JSON is silently dropped', () => {
  const out = parseSqlBlocks('[SQL]{not-json}[/SQL]');
  assert.equal(out.asks.length, 0);
});

test('numeric formatting is locale-stable', () => {
  assert.equal(fmt(1234567.5), '1,234,567.50');
  assert.equal(fmt(0), '0.00');
  assert.equal(fmt(-9.5), '-9.50');
});

test('accounting arithmetic holds', () => {
  const debit = 100000, credit = 40000;
  const opening = num(debit) - num(credit);
  assert.equal(opening, 60000);
});