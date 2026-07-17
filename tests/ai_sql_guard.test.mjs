// tests/ai_sql_guard.test.mjs
// Pure unit tests for the SQL guard — no DB required.
import { test } from 'node:test';
import assert from 'node:assert/strict';

// Inlined minimal copy of validateSql to avoid ts-node + path aliases.
// MUST match lib/ai/sql.ts::validateSql behaviour.
const ALLOWED_TABLES = new Set([
  'folio.expenses','folio.expense_items','folio.slips','folio.users',
  'folio.customers','folio.sales_orders','folio.so_items','folio.waybills',
  'folio.waybill_events','folio.chart_of_accounts','folio.ledger_lines',
  'folio.journal_entries','hr.employees','hr.leave_requests',
]);
const ALLOWED_COLUMNS = {
  'folio.expenses': new Set(['id','vendor_name','total_amount','status','submitter_id','transaction_date','created_at']),
  'folio.chart_of_accounts': new Set(['code','name','account_type']),
  'folio.ledger_lines': new Set(['id','journal_entry_id','account_code','debit','credit','description']),
  'folio.journal_entries': new Set(['id','entry_date','description','is_draft','finalized_at','finalized_by']),
  'folio.users': new Set(['id','employee_code','fullname','is_active']),
  'folio.waybills': new Set(['id','origin','origin_id','current_stage','total_amount','status','submitter_id','created_at']),
};
const FORBIDDEN = /\b(INSERT|UPDATE|DELETE|TRUNCATE|DROP|ALTER|CREATE|GRANT|REVOKE|COPY|VACUUM|REINDEX|CLUSTER|LOCK|CALL|DO\s+\$|EXPLAIN\s+ANALYZE|INTO\s+OUTFILE|LOAD\s+DATA)\b/i;
function strip(s){ return s.replace(/;+\s*$/,'').trim(); }
function aliases(sql){
  const m=new Map();
  const re=/\b(?:FROM|JOIN)\s+([a-z_][a-z0-9_.]*)(?:\s+(?:AS\s+)?([a-z_][a-z0-9_]*))?/gi;
  let r;
  while((r=re.exec(sql))!==null){
    const t=r[1].toLowerCase(); const a=(r[2]??'').toLowerCase();
    if(a){ m.set(a,t); } else {
      const i=t.lastIndexOf('.');
      const s=i>=0?t.slice(i+1):t;
      if(s && !m.has(s)) m.set(s,t);
    }
  }
  return m;
}
function validate(sql){
  const c=strip(sql);
  if(!c) return {ok:false,reason:'empty'};
  if(!/^SELECT\b/i.test(c)) return {ok:false,reason:'not select'};
  if(FORBIDDEN.test(c)) return {ok:false,reason:'forbidden'};
  const refs=Array.from(c.matchAll(/\b(?:FROM|JOIN|INTO|UPDATE)\s+([a-z_][a-z0-9_.]*)/gi)).map(m=>m[1].toLowerCase());
  if(refs.length===0) return {ok:false,reason:'no tables'};
  for(const r of refs) if(!ALLOWED_TABLES.has(r)) return {ok:false,reason:`bad table ${r}`};
  const als=aliases(c);
  const tableSet=new Set([...refs, ...als.values()]);
  for(const m of c.matchAll(/\b([a-z_][a-z0-9_]*)\.([a-z_][a-z0-9_]*)/gi)){
    const q=m[1].toLowerCase(); const col=m[2].toLowerCase();
    if(tableSet.has(q)) continue;
    if(refs.includes(`${q}.${col}`)) continue;
    const t=als.get(q) ?? q;
    const allowed=ALLOWED_COLUMNS[t];
    if(!allowed) return {ok:false,reason:`bad table ref ${q}`};
    if(!allowed.has(col)) return {ok:false,reason:`bad column ${q}.${col}`};
  }
  return {ok:true};
}

test('rejects empty SQL', () => {
  assert.equal(validate('   ').ok, false);
});

test('requires SELECT prefix', () => {
  assert.equal(validate('UPDATE folio.users SET is_active=false').ok, false);
  assert.equal(validate('INSERT INTO folio.users VALUES (1)').ok, false);
});

test('rejects forbidden keywords anywhere', () => {
  assert.equal(validate('SELECT * FROM folio.users WHERE 1=1; DROP TABLE folio.users').ok, false);
  assert.equal(validate('SELECT pg_read_file(\'/etc/passwd\')').ok, false);
});

test('rejects unknown table', () => {
  const r = validate('SELECT * FROM finance.expenses LIMIT 10');
  assert.equal(r.ok, false);
  assert.match(r.reason, /finance\.expenses/);
});

test('accepts valid query with no qualified columns', () => {
  const r = validate('SELECT id, vendor_name FROM folio.expenses LIMIT 50');
  assert.equal(r.ok, true);
});

test('accepts alias-qualified columns when alias resolves to allow-listed table', () => {
  const r = validate('SELECT e.id, e.vendor_name FROM folio.expenses AS e LIMIT 10');
  assert.equal(r.ok, true);
});

test('rejects unknown column even via alias', () => {
  const r = validate('SELECT e.password FROM folio.expenses AS e LIMIT 10');
  assert.equal(r.ok, false);
});

test('rejects alias that does not map to allow-listed table', () => {
  const r = validate('SELECT bogus.col FROM folio.expenses LIMIT 10');
  assert.equal(r.ok, false);
});

test('strips trailing semicolons', () => {
  const r = validate('SELECT id FROM folio.users LIMIT 1;;;');
  assert.equal(r.ok, true);
});