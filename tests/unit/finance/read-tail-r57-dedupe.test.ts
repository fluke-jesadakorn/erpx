import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  finPartyIdNameByCompanySql, finPeriodIdByCompanySql,
  finPartiesAllByCompanySql, finPeriodProbeByCompanySql,
} from '../../../packages/server-finance/src/adapters/finance-read-tail-r57-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-finance/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const count = (src: string, needle: string) => src.split(needle).length - 1;
const norm = (src: string) => src.replace(/\s+/g, ' ');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

type Helper = 'finPartyIdNameByCompanySql' | 'finPeriodIdByCompanySql'
  | 'finPartiesAllByCompanySql' | 'finPeriodProbeByCompanySql';
type Case = { file: string; helper: Helper; tail: string; call: string; expected: string; sha: string; label: string };

// Independent head map: the closed builders must render head + raw tail exactly.
const HEADS: Record<Helper, string> = {
  finPartyIdNameByCompanySql: 'SELECT id,name FROM finance_parties WHERE company_id=?',
  finPeriodIdByCompanySql: 'SELECT id FROM finance_periods WHERE company_id=?',
  finPartiesAllByCompanySql: 'SELECT * FROM finance_parties WHERE company_id=?',
  finPeriodProbeByCompanySql: 'SELECT 1 FROM finance_periods WHERE company_id=?',
};

const CASES: Case[] = [
  { file: "ecommerce-customers.ts", helper: 'finPartyIdNameByCompanySql', tail: " AND kind='customer' AND status='active' ORDER BY name",
    call: "finPartyIdNameByCompanySql(\" AND kind='customer' AND status='active' ORDER BY name\")", expected: "SELECT id,name FROM finance_parties WHERE company_id=? AND kind='customer' AND status='active' ORDER BY name",
    sha: 'eb267ffba1002d02dcd8b42c4c998b05d1605eff5f0eeb2645919af40dd37791', label: "ecommerce customer candidates" },
  { file: "pms-command-facts.ts", helper: 'finPartyIdNameByCompanySql', tail: " AND kind='customer' AND status='active' AND currency='THB' AND person_id IS NULL",
    call: "finPartyIdNameByCompanySql(\" AND kind='customer' AND status='active' AND currency='THB' AND person_id IS NULL\")", expected: "SELECT id,name FROM finance_parties WHERE company_id=? AND kind='customer' AND status='active' AND currency='THB' AND person_id IS NULL",
    sha: '532180fd2b13a268349acafc704a456d9fbaf178bc36cfcc19fbc0e81f6d5e78', label: "pms THB customer rows" },
  { file: "pos-daily-facts.ts", helper: 'finPartyIdNameByCompanySql', tail: " AND kind='customer' AND status='active'\n  AND (lower(trim(name))=lower(?) OR (?<>'' AND regexp_replace(COALESCE(phone,''),'[^0-9]','','g')=?)\n   OR (?<>'' AND lower(notification_email)=lower(?))) LIMIT 1",
    call: "finPartyIdNameByCompanySql(` AND kind='customer' AND status='active'\n  AND (lower(trim(name))=lower(?) OR (?<>'' AND regexp_replace(COALESCE(phone,''),'[^0-9]','','g')=?)\n   OR (?<>'' AND lower(notification_email)=lower(?))) LIMIT 1`)", expected: "SELECT id,name FROM finance_parties WHERE company_id=? AND kind='customer' AND status='active'\n  AND (lower(trim(name))=lower(?) OR (?<>'' AND regexp_replace(COALESCE(phone,''),'[^0-9]','','g')=?)\n   OR (?<>'' AND lower(notification_email)=lower(?))) LIMIT 1",
    sha: 'aeb49101b785ecf0c0c4130a3577e7e4c6c851e00e81948597316f779caaa24f', label: "pos daily fuzzy customer" },
  { file: "commerce-facts.ts", helper: 'finPeriodIdByCompanySql', tail: " AND start_date<=? AND end_date>=? AND state='open' AND (?::text IS NULL OR id=?)",
    call: "finPeriodIdByCompanySql(\" AND start_date<=? AND end_date>=? AND state='open' AND (?::text IS NULL OR id=?)\")", expected: "SELECT id FROM finance_periods WHERE company_id=? AND start_date<=? AND end_date>=? AND state='open' AND (?::text IS NULL OR id=?)",
    sha: '8d9e08d9b1bad6e3066b9a4d41bbafed0c017c69f4aacf4c42b65f2f302f2a07', label: "commerce open period scan" },
  { file: "finance-ai.ts", helper: 'finPeriodIdByCompanySql', tail: " AND (state='closing' OR (state='open' AND end_date<?)) AND NOT EXISTS(SELECT 1 FROM finance_ai_proposals p WHERE p.company_id=finance_periods.company_id AND p.source_id=finance_periods.id AND p.kind=? AND p.policy_version=? AND p.source_version=finance_periods.version) ORDER BY end_date,id LIMIT 3",
    call: "finPeriodIdByCompanySql(\" AND (state='closing' OR (state='open' AND end_date<?)) AND NOT EXISTS(SELECT 1 FROM finance_ai_proposals p WHERE p.company_id=finance_periods.company_id AND p.source_id=finance_periods.id AND p.kind=? AND p.policy_version=? AND p.source_version=finance_periods.version) ORDER BY end_date,id LIMIT 3\")", expected: "SELECT id FROM finance_periods WHERE company_id=? AND (state='closing' OR (state='open' AND end_date<?)) AND NOT EXISTS(SELECT 1 FROM finance_ai_proposals p WHERE p.company_id=finance_periods.company_id AND p.source_id=finance_periods.id AND p.kind=? AND p.policy_version=? AND p.source_version=finance_periods.version) ORDER BY end_date,id LIMIT 3",
    sha: '0849eaec2b96bb6c903e9041f220856298c15b1f58edf4aad0fce883ffe2fa16', label: "finance-ai proposal period scan" },
  { file: "manufacturing-accounting-sources.ts", helper: 'finPeriodIdByCompanySql', tail: " AND state='open' AND start_date<=? AND end_date>=? ORDER BY start_date DESC,id LIMIT 1",
    call: "finPeriodIdByCompanySql(\" AND state='open' AND start_date<=? AND end_date>=? ORDER BY start_date DESC,id LIMIT 1\")", expected: "SELECT id FROM finance_periods WHERE company_id=? AND state='open' AND start_date<=? AND end_date>=? ORDER BY start_date DESC,id LIMIT 1",
    sha: '4003bff6529aa02ba7aa584b94f536f48300d8b56de332146f08f14809657342', label: "manufacturing posting period" },
  { file: "commerce-facts.ts", helper: 'finPartiesAllByCompanySql', tail: " AND kind IN ('customer','supplier') AND person_id IS NULL ORDER BY name",
    call: "finPartiesAllByCompanySql(\" AND kind IN ('customer','supplier') AND person_id IS NULL ORDER BY name\")", expected: "SELECT * FROM finance_parties WHERE company_id=? AND kind IN ('customer','supplier') AND person_id IS NULL ORDER BY name",
    sha: 'fd78e448c13e17139248fb8065f864af4309bc2d7f997e7724969c005a71aa56', label: "commerce all-party list" },
  { file: "finance-invoice-read.ts", helper: 'finPartiesAllByCompanySql', tail: " AND status='active' AND kind IN ('customer','supplier') AND person_id IS NULL ORDER BY name",
    call: "finPartiesAllByCompanySql(\" AND status='active' AND kind IN ('customer','supplier') AND person_id IS NULL ORDER BY name\")", expected: "SELECT * FROM finance_parties WHERE company_id=? AND status='active' AND kind IN ('customer','supplier') AND person_id IS NULL ORDER BY name",
    sha: 'a7bba5f638140cfa05c01746516f0e8e8f3d7236777b2d2d848d3b8a0283abd4', label: "invoice-read workspace parties" },
  { file: "record-presentation.ts", helper: 'finPartiesAllByCompanySql', tail: " AND (id=? OR (? AND status='active' AND person_id IS NULL)) ORDER BY name,id",
    call: "finPartiesAllByCompanySql(` AND (id=? OR (? AND status='active' AND person_id IS NULL)) ORDER BY name,id`)", expected: "SELECT * FROM finance_parties WHERE company_id=? AND (id=? OR (? AND status='active' AND person_id IS NULL)) ORDER BY name,id",
    sha: '03fe9035cfeb8db436ffa28cb3d50d643014219976d2d8479ade8a9e7103a44a', label: "record-presentation invoice parties" },
  { file: "finance-books.ts", helper: 'finPeriodProbeByCompanySql', tail: " AND id=? AND state='open' AND start_date<=? AND end_date>=?",
    call: "finPeriodProbeByCompanySql(\" AND id=? AND state='open' AND start_date<=? AND end_date>=?\")", expected: "SELECT 1 FROM finance_periods WHERE company_id=? AND id=? AND state='open' AND start_date<=? AND end_date>=?",
    sha: 'e058b3aefa41ee2049c304fcb5b4c61359b2c24713168b8179dea4862dffbeb0', label: "finance-books opening period probe" },
  { file: "finance-starter.ts", helper: 'finPeriodProbeByCompanySql', tail: " AND start_date<=? AND end_date>=?",
    call: "finPeriodProbeByCompanySql(' AND start_date<=? AND end_date>=?')", expected: "SELECT 1 FROM finance_periods WHERE company_id=? AND start_date<=? AND end_date>=?",
    sha: 'a9614765c92d2a128b7a82fe8e9dea4941bc5d928f5bca00ddce9a2f120cc530', label: "finance-starter overlapping period (bind end,start)" },
  { file: "journal-planning.ts", helper: 'finPeriodProbeByCompanySql', tail: " AND state IN ('closed','closing') AND ? BETWEEN start_date AND end_date LIMIT 1",
    call: "finPeriodProbeByCompanySql(\" AND state IN ('closed','closing') AND ? BETWEEN start_date AND end_date LIMIT 1\")", expected: "SELECT 1 FROM finance_periods WHERE company_id=? AND state IN ('closed','closing') AND ? BETWEEN start_date AND end_date LIMIT 1",
    sha: '972b71b146181c887c46c1b53e4acf0323bf870483ec58d163031bc5e83e92b0', label: "journal-planning covered period probe" },
];

const render = (cs: Case) => ({
  finPartyIdNameByCompanySql: () => finPartyIdNameByCompanySql(cs.tail),
  finPeriodIdByCompanySql: () => finPeriodIdByCompanySql(cs.tail),
  finPartiesAllByCompanySql: () => finPartiesAllByCompanySql(cs.tail),
  finPeriodProbeByCompanySql: () => finPeriodProbeByCompanySql(cs.tail),
})[cs.helper]();

// Reconstructed independently of the helper bodies: the closed byte oracle.
const oracle = (cs: Case) => HEADS[cs.helper] + cs.tail;

describe('Finance read-tail r57 closed read-tail renderers', () => {
  test('all 12 sites render byte-identically to the retired literals', () => {
    expect(CASES).toHaveLength(12);
    for (const cs of CASES) {
      const got = render(cs);
      expect(got).toBe(cs.expected);
      expect(got).toBe(oracle(cs));
      expect(sha(got)).toBe(cs.sha);
      expect(got.startsWith(HEADS[cs.helper])).toBe(true);
      expect(got.slice(HEADS[cs.helper].length)).toBe(cs.tail);
    }
    // The 12 sites span four byte-identical head families (three sites each).
    expect(CASES.filter(c => c.helper === 'finPartyIdNameByCompanySql')).toHaveLength(3);
    expect(CASES.filter(c => c.helper === 'finPeriodIdByCompanySql')).toHaveLength(3);
    expect(CASES.filter(c => c.helper === 'finPartiesAllByCompanySql')).toHaveLength(3);
    expect(CASES.filter(c => c.helper === 'finPeriodProbeByCompanySql')).toHaveLength(3);
    expect(new Set(CASES.map(c => c.expected.split('?')[0])).size).toBe(4);
    expect(new Set(CASES.map(c => c.sha)).size).toBe(12);
    expect(finPartyIdNameByCompanySql('')).toBe(HEADS.finPartyIdNameByCompanySql);
    expect(finPeriodIdByCompanySql('')).toBe(HEADS.finPeriodIdByCompanySql);
    expect(finPartiesAllByCompanySql('')).toBe(HEADS.finPartiesAllByCompanySql);
    expect(finPeriodProbeByCompanySql('')).toBe(HEADS.finPeriodProbeByCompanySql);
  });

  test('callers delegate with unchanged tails, binds and lock clauses', () => {
    for (const cs of CASES) {
      const src = read(cs.file);
      expect(norm(src)).toContain(norm(cs.call));
      expect(src).toContain(cs.tail);
      for (const q of ['"', "'", '`']) expect(src).not.toContain(q + cs.expected + q);
    }
    // finance-starter keeps the reversed bind order end,start and its chart probe.
    const starter = read('finance-starter.ts');
    expect(starter).toContain("finPeriodProbeByCompanySql(' AND start_date<=? AND end_date>=?')");
    expect(starter).toContain('.bind(actor.companyId, end, start)');
    expect(starter).toContain("'NOT EXISTS(SELECT 1 FROM finance_charts WHERE company_id=?)'");
    // Finance-books keeps its advisory lock; commerce-facts keeps its FOR UPDATE writer.
    expect(read('finance-books.ts')).toContain('FINANCE_ADVISORY_XACT_LOCK_SQL');
    expect(read('commerce-facts.ts')).toContain("prepare('SELECT id FROM finance_invoices WHERE id=? FOR UPDATE')");
    // The AI scan tail keeps its native NOT EXISTS proposal guard inline.
    expect(read('finance-ai.ts')).toContain('NOT EXISTS(SELECT 1 FROM finance_ai_proposals p');
    expect(read('record-presentation.ts')).toContain('detail.invoice.status==="draft"&&detail.invoice.source_type==="standalone"');
    expect(read('pos-daily-facts.ts')).toContain("regexp_replace(COALESCE(phone,''),'[^0-9]','','g')");
  });

  test('delegation counts match the 3+3+3+3 anchors', () => {
    const calls = (file: string, helper: Helper) => count(read(file), helper + '(');
    expect(calls('ecommerce-customers.ts', 'finPartyIdNameByCompanySql')).toBe(1);
    expect(calls('pms-command-facts.ts', 'finPartyIdNameByCompanySql')).toBe(1);
    expect(calls('pos-daily-facts.ts', 'finPartyIdNameByCompanySql')).toBe(1);
    expect(calls('commerce-facts.ts', 'finPeriodIdByCompanySql')).toBe(1);
    expect(calls('finance-ai.ts', 'finPeriodIdByCompanySql')).toBe(1);
    expect(calls('manufacturing-accounting-sources.ts', 'finPeriodIdByCompanySql')).toBe(1);
    expect(calls('commerce-facts.ts', 'finPartiesAllByCompanySql')).toBe(1);
    expect(calls('finance-invoice-read.ts', 'finPartiesAllByCompanySql')).toBe(1);
    expect(calls('record-presentation.ts', 'finPartiesAllByCompanySql')).toBe(1);
    expect(calls('finance-books.ts', 'finPeriodProbeByCompanySql')).toBe(1);
    expect(calls('finance-starter.ts', 'finPeriodProbeByCompanySql')).toBe(1);
    expect(calls('journal-planning.ts', 'finPeriodProbeByCompanySql')).toBe(1);
  });
});

describe('Finance read-tail r57 protected families and retirement', () => {
  test('r15-r56 helpers, native plans, locks and other families stay untouched', () => {
    const r54 = read('finance-read-tail-r54-sql.ts');
    expect(count(r54, 'SELECT ')).toBe(2);
    expect(r54).toContain('`SELECT to_jsonb(${alias})::text image FROM ${table} ${alias} WHERE ${tail}`');
    const r55 = read('finance-read-tail-r55-sql.ts');
    expect(count(r55, 'SELECT ')).toBe(2);
    expect(r55).toContain('`SELECT count(*) FROM ${table} WHERE company_id=?${tail}`');
    const r56 = read('finance-read-tail-r56-sql.ts');
    expect(count(r56, 'SELECT ')).toBe(3);
    expect(r56).toContain('`EXISTS(SELECT 1 FROM finance_bank_transactions t');
    expect(read('supplier-payment-money.ts')).toContain('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE');
    expect(read('payroll-other-remittance-facts.ts')).toContain('LOCK TABLE finance_charts IN SHARE MODE');
    expect(read('native-finance-read.ts')).toContain("FINANCE_RD_SUPPLIER_BATCH_LIST = 'finance_rd_supplier_batch_list'");
    const imported = [...new Set(CASES.map(c => c.file))].sort();
    expect(imported).toEqual([
      'commerce-facts.ts', 'ecommerce-customers.ts', 'finance-ai.ts', 'finance-books.ts',
      'finance-invoice-read.ts', 'finance-starter.ts', 'journal-planning.ts',
      'manufacturing-accounting-sources.ts', 'pms-command-facts.ts',
      'pos-daily-facts.ts', 'record-presentation.ts',
    ]);
    for (const file of imported) expect(file.startsWith('native-')).toBe(false);
  });

  test('each r57 helper is defined exactly once', () => {
    const helper = read('finance-read-tail-r57-sql.ts');
    for (const name of ['finPartyIdNameByCompanySql', 'finPeriodIdByCompanySql',
      'finPartiesAllByCompanySql', 'finPeriodProbeByCompanySql']) {
      expect(count(helper, 'export const ' + name + ' =')).toBe(1);
    }
    expect(count(helper, 'SELECT ')).toBe(4);
  });

  test('retirement:ts-statements pins the r61 target of 1704', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // 12 head literals collapse into four helpers (-12 literals, +4 declarations: 1718 -> 1710); r58 then 1710 -> 1708; r59 then 1708 -> 1707; r60 then 1707 -> 1705; r61 then 1705 -> 1704.
    expect(CASES).toHaveLength(12);
    expect(total).toBe(1704);
    expect(total).toBe(1718 - 14);
    expect(total).toBeLessThan(1718);
  });
});
