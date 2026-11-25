import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  ppRd51AttachmentListV2,
  ppRd51AttachmentListV1,
  ppRd51AccountingCategoryGuard,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r51-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol + '\\(', 'g')) ?? []).length;
const flat = (text: string) => text.replace(/\s+/g, ' ');

// Verbatim Files / Finance / Commerce owner projections. Byte-identity oracles only; never live query sources.
const attachments = 'SELECT "id","upload_id","owner_type","owner_id","purpose","object_key","revision","original_name","media_type","bytes","checksum_sha256","created_by","legal_hold","created_at","revoked_at","company_id","detected_media_type","source_state","scanned_at","scanner_definition_at","preview_object_key","preview_media_type","preview_checksum_sha256","preview_bytes","destroyed_at","legacy_sanitized_only" FROM attachments';
const links = 'SELECT "id","set_id","attachment_id","upload_intent_id","caption","state","supersedes_link_id","created_by","created_at","expires_at","bound_at","binding_type","binding_id","withdrawn_at","withdrawn_by","withdrawal_reason" FROM attachment_links';
const sets = 'SELECT "id","company_id","branch_id","department_id","target_type","target_id","component_kind","component_ref","purpose_key","expected_target_version","policy_version","classification","audience_json","retention_policy_key","retention_policy_version","retain_until","destruction_mode","required_count","max_files","state","created_by","created_at","updated_at","bound_at","bound_command_id" FROM attachment_sets';
const financeAccounts = 'SELECT "id","chart_id","company_id","code","name","category","parent_id","currency","is_control","status","version","created_at","updated_at","control_kind" FROM finance_accounts';
const commerceSettings = 'SELECT "company_id","chart_id","currency","receivable_account_id","payable_account_id","revenue_account_id","inventory_account_id","clearing_account_id","cogs_account_id","input_tax_account_id","output_tax_account_id","cash_account_id" FROM commerce_settings';

// Each literal is copied from the pre-r51 inline call site in canonical builder form.
// The four divergences are exactly: owner_id column, expense_payment_drafts join,
// l.state bound / draft-or-bound and ORDER BY created_at / a.created_at.
const claimV2Sql = `SELECT DISTINCT a.id,a.original_name,a.media_type,a.bytes FROM (${attachments}) a
             JOIN (${links}) l ON l.attachment_id=a.id JOIN (${sets}) s ON s.id=l.set_id
             WHERE a.owner_type='expense_claim' AND a.owner_id=? AND a.revoked_at IS NULL
               AND a.source_state='available' AND l.state='bound'
               AND s.target_type='expense_claim' AND s.target_id=a.owner_id ORDER BY a.id`;
const claimV1Sql = `SELECT id,original_name,media_type,bytes FROM (${attachments}) attachments WHERE owner_type='expense_claim' AND owner_id=? AND revoked_at IS NULL ORDER BY created_at`;
const paymentV2Sql = (readOnlyState: boolean) => `SELECT DISTINCT a.id,a.owner_id,a.original_name,a.media_type,a.bytes FROM (${attachments}) a
             JOIN expense_payment_drafts d ON d.id=a.owner_id
             JOIN (${links}) l ON l.attachment_id=a.id JOIN (${sets}) s ON s.id=l.set_id
             WHERE a.owner_type='expense_payment' AND d.expense_id=? AND d.company_id=? AND a.revoked_at IS NULL
               AND a.source_state='available' AND l.state${readOnlyState ? "='bound'" : " IN ('draft','bound')"}
               AND s.target_type='expense_payment' AND s.target_id=a.owner_id ORDER BY a.id`;
const paymentV1Sql = `SELECT a.id,a.owner_id,a.original_name,a.media_type,a.bytes FROM (${attachments}) a JOIN expense_payment_drafts d ON d.id=a.owner_id WHERE a.owner_type='expense_payment' AND d.expense_id=? AND d.company_id=? AND a.revoked_at IS NULL ORDER BY a.created_at`;
const guardSql = (categoryFirst: boolean, tail: string) =>
  `EXISTS(SELECT 1 FROM (${financeAccounts}) a JOIN (${commerceSettings}) s ON s.chart_id=a.chart_id AND s.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND ${categoryFirst ? "a.category='expense' AND a.status='active'" : "a.status='active' AND a.category='expense'"} AND ${tail})`;

const render = {
  ppRd51AttachmentListV2: (a: string[]) => ppRd51AttachmentListV2(a[0] === 'payment', a[1] === 'readOnly'),
  ppRd51AttachmentListV1: (a: string[]) => ppRd51AttachmentListV1(a[0] === 'payment'),
  ppRd51AccountingCategoryGuard: (a: string[]) => ppRd51AccountingCategoryGuard(a[0] === 'categoryFirst', a[1] as 'nullable-both' | 'account' | 'both'),
};
type Sym = keyof typeof render;
type Row = { file: string; sym: Sym; args: string[]; literal: string; binds: string[]; call: string; bindCall: string };

// One row per former inline site. Binds are the original arrays, in original order.
const oracles: Row[] = [
  {
    file: 'expenses.ts', sym: 'ppRd51AttachmentListV2', args: ['claim', 'readOnly'], literal: claimV2Sql,
    binds: ['id'], call: 'ppRd51AttachmentListV2(false, true)', bindCall: '.bind(id)',
  },
  {
    file: 'expenses.ts', sym: 'ppRd51AttachmentListV1', args: ['claim'], literal: claimV1Sql,
    binds: ['id'], call: 'ppRd51AttachmentListV1(false)', bindCall: '.bind(id)',
  },
  {
    file: 'expenses.ts', sym: 'ppRd51AttachmentListV2', args: ['payment', 'draftOrBound'], literal: paymentV2Sql(false),
    binds: ['id', 'actor.companyId'], call: 'ppRd51AttachmentListV2(true, attachmentsV2ReadOnly())', bindCall: '.bind(id, actor.companyId)',
  },
  {
    file: 'expenses.ts', sym: 'ppRd51AttachmentListV2', args: ['payment', 'readOnly'], literal: paymentV2Sql(true),
    binds: ['id', 'actor.companyId'], call: 'ppRd51AttachmentListV2(true, attachmentsV2ReadOnly())', bindCall: '.bind(id, actor.companyId)',
  },
  {
    file: 'expenses.ts', sym: 'ppRd51AttachmentListV1', args: ['payment'], literal: paymentV1Sql,
    binds: ['id', 'actor.companyId'], call: 'ppRd51AttachmentListV1(true)', bindCall: '.bind(id, actor.companyId)',
  },
  {
    file: 'expense-drafts.ts', sym: 'ppRd51AccountingCategoryGuard', args: ['categoryFirst', 'nullable-both'],
    literal: guardSql(true, '(?::text IS NULL OR (a.currency=? AND s.currency=?))'),
    binds: ['category', 'actor.companyId', 'currency', 'currency', 'currency'],
    call: "ppRd51AccountingCategoryGuard(true, 'nullable-both')", bindCall: '[category, actor.companyId, currency, currency, currency]',
  },
  {
    file: 'expense-workflow.ts', sym: 'ppRd51AccountingCategoryGuard', args: ['statusFirst', 'account'],
    literal: guardSql(false, 'a.currency=?'),
    binds: ['input.categoryAccountId', 'actor.companyId', 'row.currency'],
    call: "ppRd51AccountingCategoryGuard(false, 'account')", bindCall: '[input.categoryAccountId, actor.companyId, row.currency]',
  },
  {
    file: 'expense-workflow.ts', sym: 'ppRd51AccountingCategoryGuard', args: ['categoryFirst', 'both'],
    literal: guardSql(true, 'a.currency=? AND s.currency=?'),
    binds: ['row.category_account_id', 'actor.companyId', 'row.currency', 'row.currency'],
    call: "ppRd51AccountingCategoryGuard(true, 'both')",
    bindCall: 'row.category_account_id, actor.companyId, row.currency, row.currency',
  },
];

describe('People Payroll read-tail r51 dedupe', () => {
  test('the three shared builders own byte-identical, read-only literals and bind counts', () => {
    for (const o of oracles) {
      expect(render[o.sym](o.args), o.file + ' ' + o.call).toBe(o.literal);
      expect((o.literal.match(/\?/g) ?? []).length, o.file + ' bind arity').toBe(o.binds.length);
      readOnly(o.literal);
    }
    expect(oracles.length).toBe(8);
    // The claim list keeps bound state for either readOnly input, exactly as the former inline V2 literal.
    expect(ppRd51AttachmentListV2(false, false)).toBe(claimV2Sql);
    // No parameter may leak a lock, native kind, projection table or bind value.
    expect(src('pp-read-tail-r51-sources.ts')).not.toContain('nativePpStatement');
    expect((src('pp-read-tail-r51-sources.ts').match(/export const ppRd51/g) ?? []).length).toBe(3);
    expect((src('pp-read-tail-r51-sources.ts').match(/SELECT /g) ?? []).length).toBe(3);
  });

  test('each caller uses its builder, keeps no inline copy and keeps binds and branch', () => {
    for (const o of oracles) {
      expect(src(o.file), o.file + ' builder call').toContain(o.call);
      expect(src(o.file), o.file + ' inline copy removed').not.toContain(o.literal);
      expect(src(o.file), o.file + ' imports r51 builders').toContain("from './pp-read-tail-r51-sources'");
      expect(flat(src(o.file)), o.file + ' bind array').toContain(flat(o.bindCall));
    }
    expect(uses('expenses.ts', 'ppRd51AttachmentListV2')).toBe(2);
    expect(uses('expenses.ts', 'ppRd51AttachmentListV1')).toBe(2);
    expect(uses('expense-drafts.ts', 'ppRd51AccountingCategoryGuard')).toBe(1);
    expect(uses('expense-workflow.ts', 'ppRd51AccountingCategoryGuard')).toBe(2);
    // The attachmentsV2Readable() branch and the reads-only payment state branch stay at the consumer.
    expect(src('expenses.ts')).toContain('attachmentsV2Readable()\n          ? ppRd51AttachmentListV2(false, true)\n          : ppRd51AttachmentListV1(false),\n      )\n      .bind(id)');
    expect(src('expenses.ts')).toContain('attachmentsV2Readable()\n          ? ppRd51AttachmentListV2(true, attachmentsV2ReadOnly())\n          : ppRd51AttachmentListV1(true),\n      )\n      .bind(id, actor.companyId)');
  });

  test('locks, advisory, native kinds, cross-owner projections and r1-r50 builders stay untouched', () => {
    expect(src('payroll-adjustments.ts')).toContain('SELECT run_id FROM hr_payroll_adjustments WHERE run_id=? FOR UPDATE');
    expect(src('payroll-statutory.ts')).toContain('pg_advisory_xact_lock');
    expect(src('people-payroll.ts')).toContain('nativePpStatement');
    expect(src('pp-read-tail-r51-sources.ts')).toContain('filesProjectionSources.attachments');
    expect(src('pp-read-tail-r51-sources.ts')).toContain('financeProjectionSources.finance_accounts');
    expect(src('pp-read-tail-r51-sources.ts')).toContain('commerceProjectionSources.commerce_settings');
    expect(src('expenses.ts')).toContain("financeProjectionSources.finance_journals}) finance_journals WHERE id=? AND company_id=? AND source_type='expense_claim' AND source_id=? AND status='posted'");
    expect(src('pp-read-tail-r55-sources.ts')).toContain("a.owner_type='${owner}' AND a.owner_id${id}");
    expect(src('expenses.ts')).toContain("ppRd50SlipOwnerGuard('expense_claim')");
    expect(src('expense-workflow.ts')).toContain("financeProjectionSources.finance_periods");
    for (const name of ['expenses.ts', 'expense-drafts.ts', 'expense-workflow.ts']) {
      expect(src(name), name + ' has no foreign r51 symbol').not.toMatch(/ppRd51(?!AttachmentListV2|AttachmentListV1|AccountingCategoryGuard)/);
    }
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=1228', () => {
    const root = new URL('../../../packages/server-people_payroll/src/', import.meta.url);
    const files: URL[] = [];
    const walk = (dir: URL) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = new URL(entry.name + (entry.isDirectory() ? '/' : ''), dir);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) files.push(p);
      }
    };
    walk(root);
    const total = files.reduce((n, p) => n + (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length, 0);
    expect(total).toBeLessThan(1228);
    expect(total).toBe(1206);
  });
});
