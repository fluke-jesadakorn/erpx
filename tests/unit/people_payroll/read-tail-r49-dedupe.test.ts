import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  ppRd49ExpenseClaimReceiptV2,
  ppRd49ExpenseClaimReceiptV1,
  ppRd49AttachmentLinkPoint,
  ppRd49DraftLinkGuard,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r49-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol + '\\(', 'g')) ?? []).length;

// Verbatim Files owner projections. Byte-identity oracles only; never live query sources.
const attachments = 'SELECT "id","upload_id","owner_type","owner_id","purpose","object_key","revision","original_name","media_type","bytes","checksum_sha256","created_by","legal_hold","created_at","revoked_at","company_id","detected_media_type","source_state","scanned_at","scanner_definition_at","preview_object_key","preview_media_type","preview_checksum_sha256","preview_bytes","destroyed_at","legacy_sanitized_only" FROM attachments';
const links = 'SELECT "id","set_id","attachment_id","upload_intent_id","caption","state","supersedes_link_id","created_by","created_at","expires_at","bound_at","binding_type","binding_id","withdrawn_at","withdrawn_by","withdrawal_reason" FROM attachment_links';
const sets = 'SELECT "id","company_id","branch_id","department_id","target_type","target_id","component_kind","component_ref","purpose_key","expected_target_version","policy_version","classification","audience_json","retention_policy_key","retention_policy_version","retain_until","destruction_mode","required_count","max_files","state","created_by","created_at","updated_at","bound_at","bound_command_id" FROM attachment_sets';

const v2Sql = `EXISTS(SELECT 1 FROM (${attachments}) a JOIN (${links}) l ON l.attachment_id=a.id JOIN (${sets}) s ON s.id=l.set_id WHERE a.owner_type='expense_claim' AND a.owner_id=? AND a.revoked_at IS NULL AND a.source_state='available' AND l.state='bound' AND s.target_type='expense_claim' AND s.target_id=a.owner_id)`;
const v1Sql = `EXISTS(SELECT 1 FROM (${attachments}) attachments WHERE owner_type='expense_claim' AND owner_id=? AND revoked_at IS NULL)`;
const linkSql = (targetType: string, draft = false) =>
  `SELECT l.id,${draft ? '' : 'l.state,'}l.set_id FROM (${links}) l JOIN (${sets}) s ON s.id=l.set_id WHERE l.attachment_id=? AND s.company_id=? AND s.target_type='${targetType}' AND s.target_id=?${draft ? " AND l.state='draft'" : ''}`;
const draftGuardSql = `EXISTS(SELECT 1 FROM (${links}) attachment_links WHERE id=? AND state='draft')`;

const render = {
  ppRd49ExpenseClaimReceiptV2: (_a: string[]) => ppRd49ExpenseClaimReceiptV2(),
  ppRd49ExpenseClaimReceiptV1: (_a: string[]) => ppRd49ExpenseClaimReceiptV1(),
  ppRd49AttachmentLinkPoint: (a: string[]) => ppRd49AttachmentLinkPoint(a[0], a[1] === 'draft'),
  ppRd49DraftLinkGuard: (_a: string[]) => ppRd49DraftLinkGuard(),
};
type Sym = keyof typeof render;
type Row = { file: string; sym: Sym; args: string[]; literal: string; call: string };

// Each literal is copied from the pre-r49 inline call site in canonical builder form.
// No bind was introduced, moved or reordered; the V2/V1 branch stays at the consumer.
const oracles: Row[] = [
  { file: 'expenses.ts', sym: 'ppRd49ExpenseClaimReceiptV2', args: [], literal: v2Sql, call: 'ppRd49ExpenseClaimReceiptV2()' },
  { file: 'expenses.ts', sym: 'ppRd49ExpenseClaimReceiptV1', args: [], literal: v1Sql, call: 'ppRd49ExpenseClaimReceiptV1()' },
  { file: 'expenses.ts', sym: 'ppRd49AttachmentLinkPoint', args: ['expense_claim'], literal: linkSql('expense_claim'), call: "ppRd49AttachmentLinkPoint('expense_claim')" },
  { file: 'expenses.ts', sym: 'ppRd49AttachmentLinkPoint', args: ['expense_claim', 'draft'], literal: linkSql('expense_claim', true), call: "ppRd49AttachmentLinkPoint('expense_claim', true)" },
  { file: 'expenses.ts', sym: 'ppRd49DraftLinkGuard', args: [], literal: draftGuardSql, call: 'ppRd49DraftLinkGuard()' },
  { file: 'expense-workflow.ts', sym: 'ppRd49ExpenseClaimReceiptV2', args: [], literal: v2Sql, call: 'ppRd49ExpenseClaimReceiptV2()' },
  { file: 'expense-workflow.ts', sym: 'ppRd49ExpenseClaimReceiptV1', args: [], literal: v1Sql, call: 'ppRd49ExpenseClaimReceiptV1()' },
  { file: 'expense-payments.ts', sym: 'ppRd49AttachmentLinkPoint', args: ['expense_payment'], literal: linkSql('expense_payment'), call: "ppRd49AttachmentLinkPoint('expense_payment')" },
  { file: 'expense-payments.ts', sym: 'ppRd49DraftLinkGuard', args: [], literal: draftGuardSql, call: 'ppRd49DraftLinkGuard()' },
];

describe('People Payroll read-tail r49 four-family dedupe', () => {
  test('the four shared builders own byte-identical, read-only literals and bind counts', () => {
    expect(ppRd49ExpenseClaimReceiptV2()).toBe(v2Sql);
    expect((ppRd49ExpenseClaimReceiptV2().match(/\?/g) ?? []).length).toBe(1);
    expect(ppRd49ExpenseClaimReceiptV1()).toBe(v1Sql);
    expect((ppRd49ExpenseClaimReceiptV1().match(/\?/g) ?? []).length).toBe(1);
    expect(ppRd49AttachmentLinkPoint('expense_claim')).toBe(linkSql('expense_claim'));
    expect(ppRd49AttachmentLinkPoint('expense_payment')).toBe(linkSql('expense_payment'));
    expect(ppRd49AttachmentLinkPoint('expense_claim', true)).toBe(linkSql('expense_claim', true));
    for (const target of ['expense_claim', 'expense_payment']) {
      for (const draft of [false, true]) {
        const sql = ppRd49AttachmentLinkPoint(target, draft);
        expect((sql.match(/\?/g) ?? []).length).toBe(3);
        expect(sql).toContain(`s.target_type='${target}'`);
        expect(sql).toContain('l.attachment_id=? AND s.company_id=? AND s.target_type=');
        expect(sql.includes(" AND l.state='draft'")).toBe(draft);
      }
    }
    expect(ppRd49DraftLinkGuard()).toBe(draftGuardSql);
    expect((ppRd49DraftLinkGuard().match(/\?/g) ?? []).length).toBe(1);
    for (const o of oracles) {
      expect(render[o.sym](o.args), o.file + ' ' + o.call).toBe(o.literal);
      readOnly(o.literal);
    }
    expect(oracles.length).toBe(9);
    // Three V2, three V1, two link reads (one draft), two draft guards: 11 inline SELECTs.
    expect(6 + 3 + 2).toBe(11);
  });

  test('each caller uses its builder, keeps no inline head and keeps its binds and branch', () => {
    expect((src('pp-read-tail-r49-sources.ts').match(/export const ppRd49/g) ?? []).length).toBe(4);
    const expectedCounts: Record<string, [Sym, number][]> = {
      'expenses.ts': [
        ['ppRd49ExpenseClaimReceiptV2', 2],
        ['ppRd49ExpenseClaimReceiptV1', 2],
        ['ppRd49AttachmentLinkPoint', 2],
        ['ppRd49DraftLinkGuard', 1],
      ],
      'expense-workflow.ts': [['ppRd49ExpenseClaimReceiptV2', 1], ['ppRd49ExpenseClaimReceiptV1', 1]],
      'expense-payments.ts': [['ppRd49AttachmentLinkPoint', 1], ['ppRd49DraftLinkGuard', 1]],
    };
    for (const o of oracles) {
      expect(src(o.file), o.file + ' builder call').toContain(o.call);
      expect(src(o.file), o.file + ' inline copy removed').not.toContain(o.literal);
      expect(src(o.file), o.file + ' imports r49 builders').toContain("from './pp-read-tail-r49-sources'");
    }
    for (const [name, pairs] of Object.entries(expectedCounts)) {
      for (const [sym, count] of pairs) expect(uses(name, sym), name + ' ' + sym).toBe(count);
    }
    // The attachmentsV2Readable() ternary and the single bind stay at each receipt consumer.
    expect((src('expenses.ts').match(/\? ppRd49ExpenseClaimReceiptV2\(\)\n\s+: ppRd49ExpenseClaimReceiptV1\(\),\n\s+\[id\],/g) ?? []).length).toBe(2);
    expect((src('expense-workflow.ts').match(/\? ppRd49ExpenseClaimReceiptV2\(\)\n\s+: ppRd49ExpenseClaimReceiptV1\(\),\n\s+\[id\],/g) ?? []).length).toBe(1);
    // Three binds in attachmentId/companyId/id order on every link point read.
    expect((src('expenses.ts').match(/\.bind\(attachmentId, actor\.companyId, id\)/g) ?? []).length).toBe(2);
    expect((src('expense-payments.ts').match(/\.bind\(attachmentId, actor\.companyId, id\)/g) ?? []).length).toBe(1);
    expect(src('expenses.ts')).toContain("write.prepare(ppRd49AttachmentLinkPoint('expense_claim', true))");
    expect(src('expense-payments.ts')).toContain("write.prepare(ppRd49AttachmentLinkPoint('expense_payment'))");
  });

  test('locks, advisory, native kinds, cross-owner projections and r1-r48 builders stay untouched', () => {
    expect(src('payroll-adjustments.ts')).toContain('SELECT run_id FROM hr_payroll_adjustments WHERE run_id=? FOR UPDATE');
    expect(src('payroll-adjustments.ts')).toContain('SELECT id FROM hr_time_entries WHERE id=? FOR SHARE');
    expect(src('payroll-statutory.ts')).toContain('lockProfile(value: string) { return database.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))")');
    expect(src('payroll-statutory.ts')).toContain('lockYtd(value: string) { return database.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))")');
    expect(src('people-payroll.ts')).toContain('nativePpStatement');
    expect(src('expense-workflow.ts')).toContain('financeProjectionSources.finance_accounts');
    expect(src('expenses.ts')).toContain("financeProjectionSources.finance_journals}) finance_journals WHERE id=? AND company_id=? AND source_type='expense_claim' AND source_id=? AND status='posted'");
    expect(src('expense-payments.ts')).toContain("a.owner_type='expense_payment' AND a.owner_id=?");
    expect(src('expenses.ts')).toContain("WHERE id=? AND owner_id=? AND owner_type='expense_claim' AND revoked_at IS NULL");
    expect(src('expenses.ts')).toContain("SELECT id,original_name,media_type,bytes FROM (${filesProjectionSources.attachments}) attachments WHERE owner_type='expense_claim'");
    expect(src('pp-read-tail-r48-sources.ts')).toContain('hospitalityOwnerProjectionSources as financeProjectionSources');
    expect(src('pp-read-tail-r48-sources.ts')).toContain('${financeProjectionSources.finance_invoices}');
    expect(src('pp-read-tail-r47-sources.ts')).toContain('export const ppRd47GraphSnapshotProjection');
    expect(src('pp-read-tail-r49-sources.ts')).toContain('hospitalityOwnerProjectionSources as filesProjectionSources');
    for (const name of ['expenses.ts', 'expense-workflow.ts', 'expense-payments.ts']) {
      expect(src(name), name + ' has no foreign r49 symbol').not.toMatch(/ppRd49(?!ExpenseClaimReceiptV2|ExpenseClaimReceiptV1|AttachmentLinkPoint|DraftLinkGuard)/);
    }
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=1240', () => {
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
    expect(total).toBeLessThan(1240);
    expect(total).toBe(1233);
  });
});
