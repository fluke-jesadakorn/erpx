import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));
import { manufacturingFinanceRecordSources } from '../../../packages/server-manufacturing/src/adapters/finance-record-sources';
import { manufacturingSumSelect } from '../../../packages/server-manufacturing/src/adapters/manufacturing-read-tail-r35-sql';

const adapter = (name: string) => new URL(`../../../packages/server-manufacturing/src/adapters/${name}`, import.meta.url);
const publicSrc = (name: string) => new URL(`../../../packages/server-manufacturing/src/public/${name}`, import.meta.url);
const sha256 = (text: string) => createHash('sha256').update(text).digest('hex');

/** Byte oracles captured from the pre-fold HEAD balance subqueries. The debit/
 * credit line-sum witness and the production cost-movement sum opened the same
 * byte-identical `(SELECT sum(` head, with only the summed column and the
 * FROM/WHERE tail varying; the r35 emitter renders each verbatim on the
 * caller's own prepared statement. The SHA-256 values are byte-identity
 * oracles only, never live query sources. */
const DEBIT = '(SELECT sum(debit_minor) FROM finance_journal_lines WHERE journal_id=journal.id)';
const CREDIT = '(SELECT sum(credit_minor) FROM finance_journal_lines WHERE journal_id=journal.id)';
const COST_MOVEMENTS = "(SELECT sum(amount_minor) FROM manufacturing_cost_movements movement WHERE movement.company_id=posting.company_id AND movement.order_id=production.id AND movement.source_type='production_event' AND movement.source_id=event.id AND movement.journal_id=journal.id)";

const FAMILY: readonly {
  name: string;
  column: string;
  tail: string;
  oracle: string;
  sha256: string;
}[] = [
  {
    name: 'journal/debit',
    column: 'debit_minor',
    tail: ' FROM finance_journal_lines WHERE journal_id=journal.id',
    oracle: DEBIT,
    sha256: '4a0e186c481285cb5e5ab500d46ce481a14b3e8458cc74655496e4953959a93e',
  },
  {
    name: 'journal/credit',
    column: 'credit_minor',
    tail: ' FROM finance_journal_lines WHERE journal_id=journal.id',
    oracle: CREDIT,
    sha256: 'f3f2e5fca618add1b0464fe1b650b73cb48cb615c26ae243e281f120f4e39517',
  },
  {
    name: 'posting/cost-movements',
    column: 'amount_minor',
    tail: " FROM manufacturing_cost_movements movement WHERE movement.company_id=posting.company_id AND movement.order_id=production.id AND movement.source_type='production_event' AND movement.source_id=event.id AND movement.journal_id=journal.id",
    oracle: COST_MOVEMENTS,
    sha256: '10930bc23bedf45f331306d8eec007641a34065f4af20049c488e484cc45eaaa',
  },
];

/** Post-fold whole-file pin proving the caller's binds, prepared statements and
 * surrounding SQL are byte-unchanged beyond the delegated sum subqueries. */
const FINANCE_AFTER_SHA256 = '1940f9f739ebdd8d82cd3b1d9c7efff88a227a5aeef132d79680ffaa1f1d331b';
/** Lock-bearing, native and cross-owner witnesses are untouched by r35. */
const NATIVE_COMMAND_SHA256 = '51cda9ad07a4ecbdd6c098f1b2e859a947b8309cea74dfec26fe975b842b25a7';
const COMMAND_CALLER_SHA256 = '9714b3ac0a43d89829d97fdbbf72c24ab13913d739c5ea80a24f2a16d371610a';

async function tsFiles(directory: URL): Promise<URL[]> {
  const found: URL[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory));
  }
  return found;
}

describe('manufacturing read-tail r35 sum-subquery head-family fold', () => {
  it('renders all three former balance subqueries byte-identically to their pre-fold oracles', () => {
    for (const { name, column, tail, oracle, sha256: expected } of FAMILY) {
      expect(sha256(oracle), name).toBe(expected);
      expect(manufacturingSumSelect(column, tail), name).toBe(oracle);
      expect(oracle, name).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('keeps both exported posting statements byte-identical with all three oracles', () => {
    for (const [name, sql] of Object.entries(manufacturingFinanceRecordSources)) {
      for (const { name: family, oracle } of FAMILY) expect(sql, `${name}/${family}`).toContain(oracle);
      expect(sql, name).toContain('JOIN finance_journals journal ON journal.id=posting.journal_id');
    }
  });

  it('defines one text-only emitter and routes both callers through it', async () => {
    const helper = await readFile(adapter('manufacturing-read-tail-r35-sql.ts'), 'utf8');
    expect(helper.split('manufacturingSumSelect').length - 1).toBe(1);
    expect(helper.split('SELECT').length - 1).toBe(1);
    expect(helper).not.toMatch(/prepare\(|\.run\(|\.all\(|\.first\(/);

    const finance = await readFile(adapter('finance-record-sources.ts'), 'utf8');
    expect(finance).toContain("import {manufacturingSumSelect} from './manufacturing-read-tail-r35-sql'");
    expect(finance.match(/manufacturingSumSelect\(/g) ?? []).toHaveLength(2);
    expect(finance).toContain('const manufacturingJournalLineSum=(column:string)=>manufacturingSumSelect(column," FROM finance_journal_lines WHERE journal_id=journal.id")');
    expect(finance).not.toContain('(SELECT sum(');
    expect(finance.split('const manufacturingJournalSelect=')).toHaveLength(2);
    expect(finance.split('manufacturingJournalSelect(').length - 1).toBe(2);
    expect(finance.match(/prepare\(/g) ?? []).toHaveLength(2); // only the two LOCK TABLE statements
  });

  it('leaves the caller byte-unchanged beyond the delegated subqueries (binds included)', async () => {
    const finance = await readFile(adapter('finance-record-sources.ts'), 'utf8');
    expect(sha256(finance)).toBe(FINANCE_AFTER_SHA256);
    // The outer statement keeps every bind and projection byte for byte.
    expect(finance).toContain('`SELECT ${columns} FROM manufacturing_order_postings posting');
    expect(finance).toContain('WHERE posting.amount_minor>0 AND event.action=CASE posting.phase');
  });

  it('leaves locks/native/cross-owner witnesses untouched', async () => {
    expect(sha256(await readFile(publicSrc('native_command.rs'), 'utf8'))).toBe(NATIVE_COMMAND_SHA256);
    const caller = await readFile(adapter('production-command-sources.ts'), 'utf8');
    expect(sha256(caller)).toBe(COMMAND_CALLER_SHA256);
    expect(caller).toContain('${manufacturingIdSelect("manufacturing_orders")} WHERE id=? FOR UPDATE');
    const finance = await readFile(adapter('finance-record-sources.ts'), 'utf8');
    expect(finance).toContain('LOCK TABLE planning_scenarios,planning_recommendations');
    expect(finance).toContain('LOCK TABLE manufacturing_specifications,manufacturing_specification_lines');
    const helper = await readFile(adapter('manufacturing-read-tail-r35-sql.ts'), 'utf8');
    expect(helper).not.toMatch(/LOCK\s+TABLE|FOR\s+UPDATE|FOR\s+SHARE|pg_advisory/i);
  });

  it('pins the manufacturing owner SELECT KPI at 45 after the r22 native detail-read cut (46->45)', async () => {
    let total = 0;
    for (const file of await tsFiles(new URL('../../../packages/server-manufacturing/src/', import.meta.url))) {
      total += (await readFile(file, 'utf8')).split('SELECT').length - 1;
    }
    expect(total).toBe(45);
    expect(total).toBeLessThan(50);
  });
});
