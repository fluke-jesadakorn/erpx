import 'server-only';
import type { QueryResult, QueryResultRow } from 'pg';
import { query, withTransaction } from '@/db';
import { matchPerm } from '@/perm/grammar';
import type { JournalDraft, JournalLine, JournalRecord, PostingActor } from './types';

export type FinanceQuery = <R extends QueryResultRow = QueryResultRow>(text: string, params?: unknown[]) => Promise<QueryResult<R>>;
type Q = FinanceQuery;

interface JournalRow {
  id: number;
  journal_no: string | null;
  status: JournalRecord['status'];
  posting_date: string;
  document_date: string;
  description: string;
  currency_code: string;
  fx_rate: string;
  source_type: string;
  source_id: string;
  source_event_key: string;
  branch_id: string;
  waybill_id: string | null;
  preparer_id: number | null;
  approver_id: number | null;
  reversal_of_id: string | null;
}

interface LineRow {
  account_code: string;
  description: string;
  debit_thb: string;
  credit_thb: string;
  foreign_amount: string | null;
  currency_code: string | null;
  branch_id: string;
  department_id: string | null;
  customer_id: number | null;
  vendor_id: string | null;
  employee_id: number | null;
  product_id: string | null;
  warehouse_id: string | null;
  waybill_id: string | null;
  source_document_type: string | null;
  source_document_id: string | null;
  metadata: Record<string, unknown>;
}

export class PostingError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'PostingError';
  }
}

function can(actor: PostingActor, permission: string) {
  return matchPerm(actor.permissions, permission) || matchPerm(actor.permissions, 'admin:system:bypass::allow');
}

function requirePerm(actor: PostingActor, permission: string) {
  if (!can(actor, permission)) throw new PostingError(`Missing permission: ${permission}`);
}

function cents(value: number | undefined) {
  const n = Number(value ?? 0);
  if (!Number.isFinite(n)) throw new PostingError('Journal amounts must be finite numbers');
  return Math.round(n * 100) / 100;
}

function validateDraft(draft: JournalDraft) {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(draft.postingDate)) throw new PostingError('Posting date must be YYYY-MM-DD');
  if (!draft.sourceEventKey.trim()) throw new PostingError('A source event key is required');
  if (!Number.isInteger(draft.branchId) || draft.branchId <= 0) throw new PostingError('A valid branch is required');
  if (draft.lines.length < 2) throw new PostingError('A journal requires at least two lines');
  let debit = 0;
  let credit = 0;
  draft.lines.forEach((line, index) => {
    const d = cents(line.debitThb);
    const c = cents(line.creditThb);
    if ((d > 0) === (c > 0)) throw new PostingError(`Line ${index + 1} must contain exactly one debit or credit`);
    if (d < 0 || c < 0) throw new PostingError(`Line ${index + 1} cannot be negative`);
    if (!line.accountCode.trim()) throw new PostingError(`Line ${index + 1} requires an account`);
    debit += d;
    credit += c;
  });
  if (cents(debit) !== cents(credit)) throw new PostingError(`Journal is not balanced: ${cents(debit)} debit / ${cents(credit)} credit`);
}

function mapLine(row: LineRow): JournalLine {
  return {
    accountCode: row.account_code,
    description: row.description,
    debitThb: Number(row.debit_thb),
    creditThb: Number(row.credit_thb),
    foreignAmount: row.foreign_amount === null ? null : Number(row.foreign_amount),
    currencyCode: row.currency_code,
    branchId: Number(row.branch_id),
    departmentId: row.department_id,
    customerId: row.customer_id,
    vendorId: row.vendor_id === null ? null : Number(row.vendor_id),
    employeeId: row.employee_id,
    productId: row.product_id === null ? null : Number(row.product_id),
    warehouseId: row.warehouse_id === null ? null : Number(row.warehouse_id),
    waybillId: row.waybill_id,
    sourceDocumentType: row.source_document_type,
    sourceDocumentId: row.source_document_id,
    metadata: row.metadata,
  };
}

async function loadWith(q: Q, id: number): Promise<JournalRecord> {
  const journal = await q<JournalRow>(
    `SELECT id, journal_no, status, posting_date::text, document_date::text, description,
            currency_code, fx_rate::text, source_type, source_id, source_event_key,
            branch_id::text, waybill_id, preparer_id, approver_id, reversal_of_id::text
       FROM finance.journals WHERE id = $1`,
    [id],
  );
  const row = journal.rows[0];
  if (!row) throw new PostingError(`Journal ${id} not found`);
  const lines = await q<LineRow>(
    `SELECT account_code, description, debit_thb::text, credit_thb::text,
            foreign_amount::text, currency_code, branch_id::text, department_id,
            customer_id, vendor_id::text, employee_id, product_id::text,
            warehouse_id::text, waybill_id, source_document_type, source_document_id, metadata
       FROM finance.journal_lines WHERE journal_id = $1 ORDER BY line_no`,
    [id],
  );
  return {
    id: row.id,
    journalNo: row.journal_no,
    status: row.status,
    postingDate: row.posting_date,
    documentDate: row.document_date,
    description: row.description,
    currencyCode: row.currency_code,
    fxRate: Number(row.fx_rate),
    sourceType: row.source_type,
    sourceId: row.source_id,
    sourceEventKey: row.source_event_key,
    branchId: Number(row.branch_id),
    waybillId: row.waybill_id,
    preparerId: row.preparer_id,
    approverId: row.approver_id,
    reversalOfId: row.reversal_of_id === null ? null : Number(row.reversal_of_id),
    lines: lines.rows.map(mapLine),
  };
}

async function replaceLines(q: Q, journalId: number, draft: JournalDraft) {
  await q(`DELETE FROM finance.journal_lines WHERE journal_id = $1`, [journalId]);
  for (const [index, line] of draft.lines.entries()) {
    await q(
      `INSERT INTO finance.journal_lines
         (journal_id, line_no, account_code, description, debit_thb, credit_thb,
          foreign_amount, currency_code, branch_id, department_id, customer_id,
          vendor_id, employee_id, product_id, warehouse_id, waybill_id,
          source_document_type, source_document_id, metadata)
       VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)`,
      [
        journalId,
        index + 1,
        line.accountCode,
        line.description,
        cents(line.debitThb),
        cents(line.creditThb),
        line.foreignAmount ?? null,
        line.foreignAmount === undefined || line.foreignAmount === null ? null : line.currencyCode ?? draft.currencyCode ?? 'THB',
        line.branchId ?? draft.branchId,
        line.departmentId ?? null,
        line.customerId ?? null,
        line.vendorId ?? null,
        line.employeeId ?? null,
        line.productId ?? null,
        line.warehouseId ?? null,
        line.waybillId ?? draft.waybillId ?? null,
        line.sourceDocumentType ?? draft.sourceType,
        line.sourceDocumentId ?? draft.sourceId,
        line.metadata ?? {},
      ],
    );
  }
  await q(`SELECT finance.assert_journal_balanced($1)`, [journalId]);
}

async function saveWith(q: Q, draft: JournalDraft, actorId: number): Promise<JournalRecord> {
  validateDraft(draft);
  const existing = draft.id
    ? await q<{ id: number; status: string; source_event_key: string }>(
        `SELECT id, status, source_event_key FROM finance.journals WHERE id = $1 FOR UPDATE`,
        [draft.id],
      )
    : await q<{ id: number; status: string; source_event_key: string }>(
        `SELECT id, status, source_event_key FROM finance.journals WHERE source_event_key = $1 FOR UPDATE`,
        [draft.sourceEventKey],
      );
  const found = existing.rows[0];
  if (found && found.source_event_key !== draft.sourceEventKey) throw new PostingError('Journal source event key cannot be changed');
  if (found && found.status === 'posted') return loadWith(q, found.id);
  if (found && found.status !== 'draft') throw new PostingError(`Journal ${found.id} is ${found.status} and cannot be edited`);
  let id = found?.id;
  if (id) {
    await q(
      `UPDATE finance.journals
          SET posting_date = $2, document_date = $3, description = $4,
              currency_code = $5, fx_rate = $6, source_type = $7, source_id = $8,
              branch_id = $9, waybill_id = $10, attachment_keys = $11,
              metadata = $12, updated_at = now()
        WHERE id = $1`,
      [
        id,
        draft.postingDate,
        draft.documentDate ?? draft.postingDate,
        draft.description,
        draft.currencyCode ?? 'THB',
        draft.fxRate ?? 1,
        draft.sourceType,
        draft.sourceId,
        draft.branchId,
        draft.waybillId ?? null,
        draft.attachments ?? [],
        draft.metadata ?? {},
      ],
    );
  } else {
    const inserted = await q<{ id: number }>(
      `INSERT INTO finance.journals
         (posting_date, document_date, description, currency_code, fx_rate,
          source_type, source_id, source_event_key, branch_id, waybill_id,
          attachment_keys, metadata, created_by)
       VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
       RETURNING id`,
      [
        draft.postingDate,
        draft.documentDate ?? draft.postingDate,
        draft.description,
        draft.currencyCode ?? 'THB',
        draft.fxRate ?? 1,
        draft.sourceType,
        draft.sourceId,
        draft.sourceEventKey,
        draft.branchId,
        draft.waybillId ?? null,
        draft.attachments ?? [],
        draft.metadata ?? {},
        actorId,
      ],
    );
    id = inserted.rows[0].id;
  }
  await replaceLines(q, id, draft);
  return loadWith(q, id);
}

export function loadJournal(id: number) {
  return loadWith(query, id);
}

export async function saveJournal(draft: JournalDraft, actor: PostingActor) {
  return withTransaction((q) => saveWith(q, draft, actor.id));
}

export async function preparePosting(draft: JournalDraft, actor: PostingActor) {
  requirePerm(actor, 'finance:journal:prepare::allow');
  return withTransaction(async (q) => {
    const journal = await saveWith(q, draft, actor.id);
    if (journal.status === 'posted') return journal;
    await q(
      `UPDATE finance.journals
          SET status = 'prepared', preparer_id = $2, prepared_at = now(), updated_at = now()
        WHERE id = $1 AND status = 'draft'`,
      [journal.id, actor.id],
    );
    return loadWith(q, journal.id);
  });
}

export async function approveAndPostJournal(journalId: number, actor: PostingActor) {
  requirePerm(actor, 'finance:journal:approve::allow');
  return withTransaction(async (q) => {
    const current = await loadWith(q, journalId);
    if (current.status === 'posted') return current;
    if (current.status !== 'prepared') throw new PostingError('Only prepared journals can be approved');
    if (current.preparerId === actor.id) requirePerm(actor, 'finance:journal:prepare::allow');
    await q(`SELECT (finance.post_journal($1, $2)).id`, [journalId, actor.id]);
    return loadWith(q, journalId);
  });
}

export async function postJournalInTransaction(q: FinanceQuery, draft: JournalDraft, actor: PostingActor) {
  requirePerm(actor, 'finance:journal:prepare::allow');
  requirePerm(actor, 'finance:journal:approve::allow');
  const journal = await saveWith(q, draft, actor.id);
  if (journal.status === 'posted') return journal;
  await q(
    `UPDATE finance.journals
        SET status = 'prepared', preparer_id = $2, prepared_at = now(), updated_at = now()
      WHERE id = $1 AND status = 'draft'`,
    [journal.id, actor.id],
  );
  await q(`SELECT (finance.post_journal($1, $2)).id`, [journal.id, actor.id]);
  return loadWith(q, journal.id);
}

export async function loadPostingActor(q: FinanceQuery, actorId: number): Promise<PostingActor> {
  const result = await q<{ permissions: string[] }>(
    `SELECT coalesce(array_agg(DISTINCT permission_id) FILTER (WHERE permission_id IS NOT NULL), '{}') AS permissions
       FROM (
         SELECT rp.permission_id
           FROM perm.user_roles ur
           JOIN perm.role_permissions rp ON rp.role_id = ur.role_id
          WHERE ur.user_id = $1
         UNION ALL
         SELECT up.permission_id
           FROM perm.user_permissions up
          WHERE up.user_id = $1 AND up.revoked_at IS NULL
            AND (up.starts_at IS NULL OR up.starts_at <= now())
            AND (up.ends_at IS NULL OR up.ends_at > now())
       ) p`,
    [actorId],
  );
  return { id: actorId, permissions: result.rows[0]?.permissions ?? [] };
}

export async function reverseJournal(args: {
  journalId: number;
  postingDate: string;
  reason: string;
  sourceEventKey: string;
  actor: PostingActor;
}) {
  requirePerm(args.actor, 'finance:journal:reverse::allow');
  return withTransaction(async (q) => {
    const original = await loadWith(q, args.journalId);
    if (original.status !== 'posted') throw new PostingError('Only posted journals can be reversed');
    const existing = await q<{ id: number }>(
      `SELECT id FROM finance.journals WHERE source_event_key = $1`,
      [args.sourceEventKey],
    );
    if (existing.rows[0]) return loadWith(q, existing.rows[0].id);
    const reversal = await saveWith(q, {
      postingDate: args.postingDate,
      description: `Reversal of ${original.journalNo ?? `journal ${original.id}`}: ${args.reason}`,
      currencyCode: original.currencyCode,
      fxRate: original.fxRate,
      sourceType: 'journal_reversal',
      sourceId: String(original.id),
      sourceEventKey: args.sourceEventKey,
      branchId: original.branchId,
      waybillId: original.waybillId,
      metadata: { reason: args.reason, reversalOf: original.id },
      lines: original.lines.map((line) => ({
        ...line,
        debitThb: line.creditThb,
        creditThb: line.debitThb,
        description: `Reversal: ${line.description}`,
      })),
    }, args.actor.id);
    await q(
      `UPDATE finance.journals
          SET status = 'prepared', preparer_id = $2, prepared_at = now(), reversal_of_id = $3
        WHERE id = $1`,
      [reversal.id, args.actor.id, original.id],
    );
    await q(`SELECT (finance.post_journal($1, $2)).id`, [reversal.id, args.actor.id]);
    return loadWith(q, reversal.id);
  });
}

export async function postManualJournal(draft: JournalDraft, actor: PostingActor) {
  requirePerm(actor, 'finance:journal:manual::allow');
  requirePerm(actor, 'finance:journal:prepare::allow');
  requirePerm(actor, 'finance:journal:approve::allow');
  const prepared = await preparePosting({ ...draft, sourceType: 'manual' }, actor);
  return approveAndPostJournal(prepared.id, actor);
}

export async function voidJournal(journalId: number, actor: PostingActor) {
  requirePerm(actor, 'finance:journal:prepare::allow');
  return withTransaction(async (q) => {
    const current = await loadWith(q, journalId);
    if (current.status === 'posted') throw new PostingError('Posted journals must be reversed');
    await q(
      `UPDATE finance.journals
          SET status = 'void', voided_by = $2, voided_at = now(), updated_at = now()
        WHERE id = $1 AND status IN ('draft', 'prepared')`,
      [journalId, actor.id],
    );
    return loadWith(q, journalId);
  });
}

export async function listJournals(args: {
  status?: JournalRecord['status'];
  dateFrom?: string;
  dateTo?: string;
  branchId?: number;
  limit?: number;
} = {}) {
  const result = await query<{ id: number }>(
    `SELECT id FROM finance.journals
      WHERE ($1::text IS NULL OR status = $1)
        AND ($2::date IS NULL OR posting_date >= $2)
        AND ($3::date IS NULL OR posting_date <= $3)
        AND ($4::bigint IS NULL OR branch_id = $4)
      ORDER BY posting_date DESC, id DESC
      LIMIT $5`,
    [args.status ?? null, args.dateFrom ?? null, args.dateTo ?? null, args.branchId ?? null, Math.min(args.limit ?? 200, 500)],
  );
  return Promise.all(result.rows.map((row) => loadJournal(row.id)));
}
