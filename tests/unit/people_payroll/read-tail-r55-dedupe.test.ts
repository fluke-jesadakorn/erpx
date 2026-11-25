import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  ppRd55AttachmentPresenceV2,
  ppRd55AttachmentPresenceV1,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r55-sources';
import { hospitalityOwnerProjectionSources as F } from '@erpx/server-files/public/hospitality-owner-projection-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const flat = (text: string) => text.replace(/\s+/g, ' ');
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol + '\\(', 'g')) ?? []).length;
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

const A = F.attachments, L = F.attachment_links, S = F.attachment_sets;
const MEDIA = "'image/jpeg','image/png','image/webp','application/pdf'";

// Byte oracles copied verbatim from the six pre-r55 inline call sites at HEAD,
// with the Files projection interpolations resolved to their real runtime bytes.
const reqV2 = `SELECT 1 FROM (${A}) a JOIN (${L}) l ON l.attachment_id=a.id
             JOIN (${S}) s ON s.id=l.set_id
             WHERE a.owner_type='expense_claim' AND a.owner_id=? AND a.revoked_at IS NULL
               AND a.source_state='available' AND l.state='bound'
               AND s.target_type='expense_claim' AND s.target_id=a.owner_id LIMIT 1`;
const reqV1 = `SELECT 1 FROM (${A}) attachments WHERE owner_type='expense_claim' AND owner_id=? AND revoked_at IS NULL LIMIT 1`;
const batchV2 = `SELECT DISTINCT a.owner_id id FROM (${A}) a JOIN (${L}) l ON l.attachment_id=a.id JOIN (${S}) s ON s.id=l.set_id WHERE a.owner_type='expense_claim' AND a.owner_id IN(SELECT value FROM erpx_json_each(?)) AND a.revoked_at IS NULL AND a.source_state='available' AND l.state='bound' AND s.target_type='expense_claim' AND s.target_id=a.owner_id`;
const batchV1 = `SELECT DISTINCT owner_id id FROM (${A}) attachments WHERE owner_type='expense_claim' AND owner_id IN(SELECT value FROM erpx_json_each(?)) AND revoked_at IS NULL`;
const payV2 = `EXISTS(SELECT 1 FROM (${A}) a JOIN (${L}) l ON l.attachment_id=a.id JOIN (${S}) s ON s.id=l.set_id
        WHERE a.owner_type='expense_payment' AND a.owner_id=?
          AND a.media_type IN (${MEDIA})
          AND a.revoked_at IS NULL AND a.source_state='available' AND l.state='bound'
          AND s.target_type='expense_payment' AND s.target_id=a.owner_id)`;
const payV1 = `EXISTS(SELECT 1 FROM (${A}) attachments WHERE owner_type='expense_payment' AND owner_id=? AND media_type IN (${MEDIA}) AND revoked_at IS NULL)`;

type Row = {
  file: string;
  render: () => string;
  literal: string;
  call: string;
  bindCall: string;
  inline: string;
};

// Binds are the original call-site arrays, in original order; ? count is checked.
const oracles: Row[] = [
  {
    file: 'expense-requirements.ts', render: () => ppRd55AttachmentPresenceV2('probe', false, false, false),
    literal: reqV2, call: "ppRd55AttachmentPresenceV2('probe', false, false, false)",
    bindCall: '.bind(row.id).first()', inline: '${filesProjectionSources.attachments}) a JOIN (${filesProjectionSources.attachment_links})',
  },
  {
    file: 'expense-requirements.ts', render: () => ppRd55AttachmentPresenceV1('probe', false, false, false),
    literal: reqV1, call: "ppRd55AttachmentPresenceV1('probe', false, false, false)",
    bindCall: '.bind(row.id).first()', inline: '${filesProjectionSources.attachments}) attachments WHERE owner_type=',
  },
  {
    file: 'expense-batch-facts.ts', render: () => ppRd55AttachmentPresenceV2('distinct', false, false, true),
    literal: batchV2, call: "ppRd55AttachmentPresenceV2('distinct', false, false, true)",
    bindCall: '.bind(receiptEncoded).all<{id:string}>()', inline: '${filesSources.attachments}) a JOIN (${filesSources.attachment_links})',
  },
  {
    file: 'expense-batch-facts.ts', render: () => ppRd55AttachmentPresenceV1('distinct', false, false, true),
    literal: batchV1, call: "ppRd55AttachmentPresenceV1('distinct', false, false, true)",
    bindCall: '.bind(receiptEncoded).all<{id:string}>()', inline: '${filesSources.attachments}) attachments WHERE owner_type=',
  },
  {
    file: 'expense-payments.ts', render: () => ppRd55AttachmentPresenceV2('exists', true, true, false),
    literal: payV2, call: "ppRd55AttachmentPresenceV2('exists', true, true, false)",
    bindCall: `SELECT 1 WHERE \${paymentSlipExistsSql()}\`).bind(id).first()`, inline: 'a.media_type IN (',
  },
  {
    file: 'expense-payments.ts', render: () => ppRd55AttachmentPresenceV1('exists', true, true, false),
    literal: payV1, call: "ppRd55AttachmentPresenceV1('exists', true, true, false)",
    bindCall: '.bind(id).first()', inline: 'media_type IN (',
  },
];

describe('People Payroll read-tail r55 dedupe', () => {
  test('the two shared builders render six byte-identical, read-only presence probes', () => {
    for (const o of oracles) {
      expect(o.render(), o.file + ' ' + o.call).toBe(o.literal);
      expect((o.literal.match(/\?/g) ?? []).length, o.file + ' bind arity').toBe(1);
      readOnly(o.literal);
    }
    expect(oracles.length).toBe(6);
    // Exact V2/V1 shapes, owner kinds and media/scalar/json tails are preserved.
    expect(ppRd55AttachmentPresenceV2('list', false, false, true)).toBe(batchV2);
    expect(ppRd55AttachmentPresenceV1('list', false, false, true)).toBe(batchV1);
    expect(reqV2).toContain('a.source_state=\'available\' AND l.state=\'bound\'');
    expect(payV2).toContain(`AND a.media_type IN (${MEDIA})`);
    expect(batchV2).toContain('IN(SELECT value FROM erpx_json_each(?))');
    expect(reqV1).not.toContain('attachment_links');
    // Single V2/V1 pair, exactly four SELECT tokens, no lock or native leakage.
    const helper = src('pp-read-tail-r55-sources.ts');
    expect((helper.match(/export const ppRd55/g) ?? []).length).toBe(2);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(4);
    expect(helper).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|pg_advisory|nativePpStatement/i);
  });

  test('all six callers delegate to the builders and keep no inline copy or bind change', () => {
    for (const o of oracles) {
      expect(src(o.file), o.file + ' builder call').toContain(o.call);
      expect(src(o.file), o.file + ' inline copy removed').not.toContain(o.inline);
      expect(src(o.file), o.file + ' imports r55 builders').toContain("from './pp-read-tail-r55-sources'");
      expect(flat(src(o.file)), o.file + ' bind call').toContain(flat(o.bindCall));
    }
    expect(uses('expense-requirements.ts', 'ppRd55AttachmentPresenceV2')).toBe(1);
    expect(uses('expense-requirements.ts', 'ppRd55AttachmentPresenceV1')).toBe(1);
    expect(uses('expense-batch-facts.ts', 'ppRd55AttachmentPresenceV2')).toBe(1);
    expect(uses('expense-batch-facts.ts', 'ppRd55AttachmentPresenceV1')).toBe(1);
    expect(uses('expense-payments.ts', 'ppRd55AttachmentPresenceV2')).toBe(1);
    expect(uses('expense-payments.ts', 'ppRd55AttachmentPresenceV1')).toBe(1);
    for (const name of ['expense-requirements.ts', 'expense-batch-facts.ts', 'expense-payments.ts']) {
      expect(src(name), name + ' has no foreign r55 symbol').not.toMatch(/ppRd55(?!AttachmentPresenceV2|AttachmentPresenceV1)/);
    }
    // The V2/V1 branch and paymentSlipExistsSql wrapper stay at the caller.
    expect(src('expense-payments.ts')).toContain('function paymentSlipExistsSql() {');
    expect(src('expense-payments.ts')).toContain('return attachmentsV2Readable()');
  });

  test('locks, advisory, native kinds and cross-owner tails stay byte-identical and untouched', () => {
    expect(src('payroll-adjustments.ts')).toContain('SELECT run_id FROM hr_payroll_adjustments WHERE run_id=? FOR UPDATE');
    expect(src('payroll-statutory.ts')).toContain('pg_advisory_xact_lock');
    expect(src('people-evaluation.ts')).toContain('pg_advisory_xact_lock');
    expect(src('people-payroll.ts')).toContain('nativePpStatement');
    // The unique-payment-slip cross-owner projection tail remains inline at its caller.
    expect(src('expense-payments.ts')).toContain("used.owner_type='expense_payment' AND used.revoked_at IS NULL");
    expect(src('expense-payments.ts')).toContain("current.owner_type='expense_payment' AND current.owner_id=?");
    // r49/r51/r52 families stay byte-identical and reusable.
    expect(src('pp-read-tail-r49-sources.ts')).toContain('export const ppRd49ExpenseClaimReceiptV2');
    expect(src('pp-read-tail-r52-sources.ts')).toContain('export const ppRd52PaymentImage');
    expect(src('pp-read-tail-r51-sources.ts')).toContain('filesProjectionSources.attachments');
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=1214', () => {
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
    expect(total).toBeLessThan(1214);
    expect(total).toBe(1195);
  });
});
