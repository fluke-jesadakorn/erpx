import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
vi.mock('server-only', () => ({}));
import {
  nativeProcurementReadAll,
  nativeProcurementReadFirst,
  PROCUREMENT_RD_INVOICE_MATCH_FOR_FINANCE,
  PROCUREMENT_RD_REQUEST_QUOTATION,
  PROCUREMENT_RD_OPERATIONAL_ATTACHMENT_BILL_MATCH,
  PROCUREMENT_RD_DOCUMENT_POSTING_MATCHED_BILL,
  PROCUREMENT_RD_BILL_MATCH_RECEIPTS,
  PROCUREMENT_RD_ACTIVE_QUOTATION_COUNT,
  PROCUREMENT_RD_REQUEST_ALLOCATIONS,
  PROCUREMENT_RD_REQUEST_ORDER_LINK,
  PROCUREMENT_RD_BILL_MATCH_VARIANCE,
  PROCUREMENT_RD_BILL_MATCH_RECEIPT_HISTORY,
  PROCUREMENT_RD_INVOICE_POLICY_MATCHING_REQUIRED,
  PROCUREMENT_RD_RECORD_BILL_MATCH_EXISTS,
  PROCUREMENT_RD_RECORD_RFQ_EXISTS,
  PROCUREMENT_RD_BILL_MATCH_EVENTS,
  PROCUREMENT_RD_BILL_MATCH_LINES_FOR_INVOICE,
  PROCUREMENT_RD_POSTED_BILL_MATCH_FOR_INVOICE,
  PROCUREMENT_RD_EMPLOYEE_REQUEST_ORDERED_QUOTATION,
  PROCUREMENT_RD_HANDOFF_APPROVED_QUOTATION,
  PROCUREMENT_RD_PROGRESS_REQUEST_RFQS,
  PROCUREMENT_RD_RFQ_BY_ORDER,
  PROCUREMENT_RD_BILL_BY_INVOICE,
  PROCUREMENT_RD_LATEST_BILL_BY_FULFILLMENT,
} from '../../../packages/server-procurement/src/adapters/native-procurement-read';
import { procurementInvoiceMatchForFinance } from '../../../packages/server-procurement/src/adapters/finance-entrypoint-queries';
import { requestQuotation } from '../../../packages/server-procurement/src/adapters/people-app-entrypoint-facts';
import { OperationalAttachmentProcurementFacts } from '../../../packages/server-procurement/src/adapters/attachments-operational-facts';
import { documentPostingPreviewFacts } from '../../../packages/server-procurement/src/adapters/document-posting-preview-facts';
import { EmployeeRequestsProcurementOwnedFacts } from '../../../packages/server-procurement/src/adapters/records-employee-requests-owned-facts';
import { CommerceProcurementFacts } from '../../../packages/server-procurement/src/adapters/commerce-facts';

const envKey = 'ERPX_NATIVE_PROCUREMENT_CONFIG';
const original = process.env[envKey];
const planned = {
  [PROCUREMENT_RD_INVOICE_MATCH_FOR_FINANCE]: 'SELECT id,reference FROM procurement_bill_matches WHERE invoice_id=? AND company_id=?',
  [PROCUREMENT_RD_REQUEST_QUOTATION]: "SELECT id FROM procurement_rfqs WHERE request_id=? AND company_id=? AND status<>'cancelled'",
  [PROCUREMENT_RD_OPERATIONAL_ATTACHMENT_BILL_MATCH]: 'SELECT company_id,branch_id,department_id,version FROM procurement_bill_matches WHERE id=? AND company_id=?',
  [PROCUREMENT_RD_DOCUMENT_POSTING_MATCHED_BILL]: "SELECT * FROM procurement_bill_matches WHERE invoice_id=? AND company_id=? AND status='invoiced'",
  [PROCUREMENT_RD_BILL_MATCH_RECEIPTS]: 'SELECT * FROM procurement_bill_match_receipts WHERE match_id=? ORDER BY id',
  [PROCUREMENT_RD_ACTIVE_QUOTATION_COUNT]: "SELECT count(*) n FROM procurement_rfqs WHERE request_id=? AND status<>'cancelled'",
  [PROCUREMENT_RD_REQUEST_ALLOCATIONS]: "SELECT l.request_item_ordinal ordinal,COALESCE(sum(l.request_quantity_milli),0) allocated_milli FROM procurement_rfq_lines l JOIN procurement_rfqs r ON r.id=l.rfq_id WHERE r.request_id=? AND r.status<>'cancelled' GROUP BY l.request_item_ordinal",
  [PROCUREMENT_RD_REQUEST_ORDER_LINK]: 'SELECT request_id FROM procurement_request_orders WHERE request_id=? AND order_id=?',
  [PROCUREMENT_RD_BILL_MATCH_VARIANCE]: 'SELECT id FROM procurement_bill_match_lines WHERE match_id=? AND subtotal_minor<>expected_subtotal_minor LIMIT 1',
  [PROCUREMENT_RD_BILL_MATCH_RECEIPT_HISTORY]: 'SELECT br.match_id id FROM procurement_bill_match_receipts br WHERE br.fulfillment_id=? LIMIT 1',
  [PROCUREMENT_RD_INVOICE_POLICY_MATCHING_REQUIRED]: "SELECT 1 FROM procurement_invoice_policies WHERE company_id=? AND matching_mode='required'",
  [PROCUREMENT_RD_RECORD_BILL_MATCH_EXISTS]: 'SELECT 1 FROM procurement_bill_matches WHERE id=?',
  [PROCUREMENT_RD_RECORD_RFQ_EXISTS]: 'SELECT 1 FROM procurement_rfqs WHERE id=?',
  [PROCUREMENT_RD_BILL_MATCH_EVENTS]: 'SELECT * FROM procurement_bill_match_events WHERE match_id=? ORDER BY version DESC',
  [PROCUREMENT_RD_BILL_MATCH_LINES_FOR_INVOICE]: "SELECT l.subtotal_minor,l.tax_minor FROM procurement_bill_match_lines l JOIN procurement_bill_matches m ON m.id=l.match_id WHERE m.invoice_id=? AND m.status='posted' AND l.fulfillment_line_id=?",
  [PROCUREMENT_RD_POSTED_BILL_MATCH_FOR_INVOICE]: "SELECT * FROM procurement_bill_matches WHERE invoice_id=? AND company_id=? AND status='posted'",
  [PROCUREMENT_RD_EMPLOYEE_REQUEST_ORDERED_QUOTATION]: "SELECT id,order_id FROM procurement_rfqs WHERE request_id=? AND order_id=? AND status='ordered'",
  [PROCUREMENT_RD_HANDOFF_APPROVED_QUOTATION]: "SELECT id FROM procurement_rfqs WHERE request_id=? AND id=? AND status='approved'",
  [PROCUREMENT_RD_PROGRESS_REQUEST_RFQS]: "SELECT id,status,order_id FROM procurement_rfqs WHERE request_id=? AND company_id=? AND status<>'cancelled' ORDER BY updated_at DESC,id DESC",
  [PROCUREMENT_RD_RFQ_BY_ORDER]: 'SELECT id FROM procurement_rfqs WHERE order_id=? AND company_id=?',
  [PROCUREMENT_RD_BILL_BY_INVOICE]: 'SELECT * FROM procurement_bill_matches WHERE invoice_id=? AND company_id=?',
  [PROCUREMENT_RD_LATEST_BILL_BY_FULFILLMENT]: "SELECT id,status FROM procurement_bill_matches WHERE fulfillment_id=? ORDER BY (status<>'returned') DESC,created_at DESC,id DESC LIMIT 1",
};
const stubFetch = (payload: unknown, status = 200) => {
  const fetchMock = vi.fn(async () => ({
    status,
    body: { cancel: vi.fn(async () => undefined) },
    json: async () => payload,
  }));
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
};

beforeEach(() => {
  process.env[envKey] = JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44997',
    timeoutMs: 1000,
    holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001', companyId: 'company' }],
  });
});
afterEach(() => {
  if (original === undefined) delete process.env[envKey];
  else process.env[envKey] = original;
  vi.unstubAllGlobals();
});

const fakeDb = (rows: Record<string, unknown>[] = [{ id: 'row-1' }]) => {
  const bound: unknown[][] = [];
  const prepared: string[] = [];
  let batched: { sql: string; values: unknown[] }[] = [];
  const db = {
    prepare(sql: string) {
      prepared.push(sql);
      return {
        bind(...values: unknown[]) { bound.push(values); return this; },
      };
    },
    async batch(statements: { __sql?: string; __values?: unknown[] }[]) {
      batched = statements.map((s) => ({ sql: s.__sql ?? prepared[0] ?? '', values: s.__values ?? bound[0] ?? [] }));
      return [{ results: rows, success: true, meta: { changes: 0, duration: 0 } }];
    },
    async exec() { return { count: 0, duration: 0 }; },
  };
  return { db, bound, prepared, get batched() { return batched; } };
};

describe('native Procurement caller-unit read plan', () => {
  it('exposes allow-listed PROCUREMENT_RD_* kinds with no SQL on the wire', () => {
    expect(PROCUREMENT_RD_INVOICE_MATCH_FOR_FINANCE).toBe('procurement_rd_invoice_match_finance');
    expect(PROCUREMENT_RD_REQUEST_QUOTATION).toBe('procurement_rd_request_quotation');
    expect(PROCUREMENT_RD_OPERATIONAL_ATTACHMENT_BILL_MATCH).toBe('procurement_rd_operational_attachment_bill_match');
    expect(PROCUREMENT_RD_DOCUMENT_POSTING_MATCHED_BILL).toBe('procurement_rd_document_posting_matched_bill');
    expect(PROCUREMENT_RD_BILL_MATCH_RECEIPTS).toBe('procurement_rd_bill_match_receipts');
    expect(PROCUREMENT_RD_ACTIVE_QUOTATION_COUNT).toBe('procurement_rd_active_quotation_count');
    expect(PROCUREMENT_RD_REQUEST_ALLOCATIONS).toBe('procurement_rd_request_allocations');
    expect(PROCUREMENT_RD_REQUEST_ORDER_LINK).toBe('procurement_rd_request_order_link');
    expect(PROCUREMENT_RD_BILL_MATCH_VARIANCE).toBe('procurement_rd_bill_match_variance');
    expect(PROCUREMENT_RD_BILL_MATCH_RECEIPT_HISTORY).toBe('procurement_rd_bill_match_receipt_history');
    expect(PROCUREMENT_RD_INVOICE_POLICY_MATCHING_REQUIRED).toBe('procurement_rd_invoice_policy_matching_required');
    expect(PROCUREMENT_RD_RECORD_BILL_MATCH_EXISTS).toBe('procurement_rd_record_bill_match_exists');
    expect(PROCUREMENT_RD_RECORD_RFQ_EXISTS).toBe('procurement_rd_record_rfq_exists');
    expect(PROCUREMENT_RD_BILL_MATCH_EVENTS).toBe('procurement_rd_bill_match_events');
    expect(PROCUREMENT_RD_BILL_MATCH_LINES_FOR_INVOICE).toBe('procurement_rd_bill_match_lines_for_invoice');
    expect(PROCUREMENT_RD_POSTED_BILL_MATCH_FOR_INVOICE).toBe('procurement_rd_posted_bill_match_for_invoice');
    expect(PROCUREMENT_RD_EMPLOYEE_REQUEST_ORDERED_QUOTATION).toBe('procurement_rd_employee_request_ordered_quotation');
    expect(PROCUREMENT_RD_HANDOFF_APPROVED_QUOTATION).toBe('procurement_rd_handoff_approved_quotation');
    expect(PROCUREMENT_RD_PROGRESS_REQUEST_RFQS).toBe('procurement_rd_progress_request_rfqs');
    expect(PROCUREMENT_RD_RFQ_BY_ORDER).toBe('procurement_rd_rfq_by_order');
    expect(PROCUREMENT_RD_BILL_BY_INVOICE).toBe('procurement_rd_bill_by_invoice');
    expect(PROCUREMENT_RD_LATEST_BILL_BY_FULFILLMENT).toBe('procurement_rd_latest_bill_by_fulfillment');
    for (const kind of [PROCUREMENT_RD_INVOICE_MATCH_FOR_FINANCE, PROCUREMENT_RD_REQUEST_QUOTATION, PROCUREMENT_RD_OPERATIONAL_ATTACHMENT_BILL_MATCH, PROCUREMENT_RD_DOCUMENT_POSTING_MATCHED_BILL, PROCUREMENT_RD_BILL_MATCH_RECEIPTS, PROCUREMENT_RD_ACTIVE_QUOTATION_COUNT, PROCUREMENT_RD_REQUEST_ALLOCATIONS, PROCUREMENT_RD_REQUEST_ORDER_LINK, PROCUREMENT_RD_BILL_MATCH_VARIANCE, PROCUREMENT_RD_BILL_MATCH_RECEIPT_HISTORY, PROCUREMENT_RD_INVOICE_POLICY_MATCHING_REQUIRED, PROCUREMENT_RD_RECORD_BILL_MATCH_EXISTS, PROCUREMENT_RD_RECORD_RFQ_EXISTS, PROCUREMENT_RD_BILL_MATCH_EVENTS, PROCUREMENT_RD_BILL_MATCH_LINES_FOR_INVOICE, PROCUREMENT_RD_POSTED_BILL_MATCH_FOR_INVOICE, PROCUREMENT_RD_EMPLOYEE_REQUEST_ORDERED_QUOTATION, PROCUREMENT_RD_HANDOFF_APPROVED_QUOTATION, PROCUREMENT_RD_PROGRESS_REQUEST_RFQS, PROCUREMENT_RD_RFQ_BY_ORDER, PROCUREMENT_RD_BILL_BY_INVOICE, PROCUREMENT_RD_LATEST_BILL_BY_FULFILLMENT]) {
      expect(kind).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
    }
  });

  it('plans the invoice-match read in Rust and runs it on the caller unit', async () => {
    const fetchMock = stubFetch({ data: { statements: [{ sql: planned[PROCUREMENT_RD_INVOICE_MATCH_FOR_FINANCE], values: ['inv-1', 'company'] }] } });
    const { db } = fakeDb([{ id: 'bill-1', reference: 'INV-1' }]);
    const row = await nativeProcurementReadFirst<{ id: string; reference: string }>(
      PROCUREMENT_RD_INVOICE_MATCH_FOR_FINANCE, ['inv-1', 'company'], db as never,
    );
    expect(row).toEqual({ id: 'bill-1', reference: 'INV-1' });
    const [url, init] = fetchMock.mock.calls[0] as unknown as [URL, RequestInit];
    expect(String(url)).toContain('/api/v1/holdings/acme/procurement/command-plan');
    const body = JSON.parse(init.body as string) as { kind: string; values: unknown[] };
    expect(body.kind).toBe('procurement_rd_invoice_match_finance');
    expect(body.values).toEqual(['inv-1', 'company']);
    expect(JSON.stringify(body)).not.toContain('SELECT');
  });

  it('switched finance entrypoint adapter carries no SQL and returns the planned row', async () => {
    stubFetch({ data: { statements: [{ sql: planned[PROCUREMENT_RD_INVOICE_MATCH_FOR_FINANCE], values: ['inv-1', 'company'] }] } });
    const { db, prepared, bound } = fakeDb([{ id: 'bill-1', reference: 'INV-1' }]);
    const row = await procurementInvoiceMatchForFinance(db as never, 'inv-1', 'company');
    expect(row).toEqual({ id: 'bill-1', reference: 'INV-1' });
    expect(prepared).toEqual([planned[PROCUREMENT_RD_INVOICE_MATCH_FOR_FINANCE]]);
    expect(bound).toEqual([['inv-1', 'company']]);
  });

  it('switched people-app entrypoint adapter carries no SQL and returns the planned row', async () => {
    stubFetch({ data: { statements: [{ sql: planned[PROCUREMENT_RD_REQUEST_QUOTATION], values: ['req-1', 'company'] }] } });
    const { db, prepared, bound } = fakeDb([{ id: 'rfq-1' }]);
    const row = await requestQuotation(db as never, 'req-1', 'company');
    expect(row).toEqual({ id: 'rfq-1' });
    expect(prepared).toEqual([planned[PROCUREMENT_RD_REQUEST_QUOTATION]]);
    expect(bound).toEqual([['req-1', 'company']]);
  });

  it('switched attachments-operational facts adapter carries no SQL and returns the planned row', async () => {
    const row = { company_id: 'company', branch_id: null, department_id: null, version: 3 };
    stubFetch({ data: { statements: [{ sql: planned[PROCUREMENT_RD_OPERATIONAL_ATTACHMENT_BILL_MATCH], values: ['bill-1', 'company'] }] } });
    const { db, prepared, bound } = fakeDb([row]);
    const facts = OperationalAttachmentProcurementFacts(db as never);
    const result = await facts.readProcurementBillMatches1({ targetId: 'bill-1', companyId: 'company' });
    expect(result).toEqual(row);
    expect(prepared).toEqual([planned[PROCUREMENT_RD_OPERATIONAL_ATTACHMENT_BILL_MATCH]]);
    expect(bound).toEqual([['bill-1', 'company']]);
  });

  it('switched document-posting-preview facts adapter carries no SQL and returns the planned lines', async () => {
    const bill = {
      expected_subtotal_minor: 1000, subtotal_minor: 1000, tax_minor: 70,
      clearing_account_id: 'clearing', variance_account_id: 'variance',
      tax_account_id: 'tax', payable_account_id: 'payable',
    };
    stubFetch({ data: { statements: [{ sql: planned[PROCUREMENT_RD_DOCUMENT_POSTING_MATCHED_BILL], values: ['bill-1', 'company'] }] } });
    const { db, prepared, bound } = fakeDb([bill]);
    const lines = await documentPostingPreviewFacts(db as never).matchedInvoiceLines('company', 'bill-1');
    expect(lines).toEqual([
      { id: 'clearing', debit: 1000, credit: 0 },
      { id: 'tax', debit: 70, credit: 0 },
      { id: 'payable', debit: 0, credit: 1070 },
    ]);
    expect(prepared).toEqual([planned[PROCUREMENT_RD_DOCUMENT_POSTING_MATCHED_BILL]]);
    expect(bound).toEqual([['bill-1', 'company']]);
  });

  it('plans the bill-match receipts read and retires the supplier-bills SELECT', async () => {
    const sql = planned[PROCUREMENT_RD_BILL_MATCH_RECEIPTS];
    const fetchMock = stubFetch({ data: { statements: [{ sql, values: ['bill-1'] }] } });
    const { db, prepared, bound } = fakeDb([{ id: 'receipt-1', match_id: 'bill-1' }]);
    const rows = await nativeProcurementReadAll<Record<string, unknown>>(
      PROCUREMENT_RD_BILL_MATCH_RECEIPTS, ['bill-1'], db as never,
    );
    expect(rows.results).toEqual([{ id: 'receipt-1', match_id: 'bill-1' }]);
    expect(prepared).toEqual([sql]);
    expect(bound).toEqual([['bill-1']]);
    const [, init] = fetchMock.mock.calls[0] as unknown as [URL, RequestInit];
    const body = JSON.parse(init.body as string) as { kind: string; values: unknown[] };
    expect(body.kind).toBe('procurement_rd_bill_match_receipts');
    expect(body.values).toEqual(['bill-1']);
    expect(JSON.stringify(body)).not.toContain('SELECT');

    const source = readFileSync(
      new URL('../../../packages/server-procurement/src/adapters/supplier-bills.ts', import.meta.url),
      'utf8',
    );
    expect(source).not.toContain('SELECT * FROM procurement_bill_match_receipts WHERE match_id=? ORDER BY id');
    expect(source).toContain('PROCUREMENT_RD_BILL_MATCH_RECEIPTS');
  });

  it('plans the active-quotation-count read and retires both TypeScript literals', async () => {
    const sql = planned[PROCUREMENT_RD_ACTIVE_QUOTATION_COUNT];
    const fetchMock = stubFetch({ data: { statements: [{ sql, values: ['req-1'] }] } });
    const { db, prepared, bound } = fakeDb([{ n: 2 }]);
    const facts = EmployeeRequestsProcurementOwnedFacts(db as never);
    expect(await facts.readActOnEmployeeRequestQuotationCount('req-1')).toBe(2);
    expect(await facts.readPrepareRequestOrderHandoffActiveQuotationCount('req-1')).toBe(2);
    expect(prepared).toEqual([sql, sql]);
    expect(bound).toEqual([['req-1'], ['req-1']]);
    const [, init] = fetchMock.mock.calls[0] as unknown as [URL, RequestInit];
    const body = JSON.parse(init.body as string) as { kind: string; values: unknown[] };
    expect(body.kind).toBe('procurement_rd_active_quotation_count');
    expect(body.values).toEqual(['req-1']);
    expect(JSON.stringify(body)).not.toContain('SELECT');

    const source = readFileSync(
      new URL('../../../packages/server-procurement/src/adapters/records-employee-requests-owned-facts.ts', import.meta.url),
      'utf8',
    );
    expect(source).not.toContain("SELECT count(*) n FROM procurement_rfqs WHERE request_id=? AND status<>'cancelled'");
    expect(source).toContain('PROCUREMENT_RD_ACTIVE_QUOTATION_COUNT');
  });

  it('plans the request-allocation read and retires all three TypeScript literals', async () => {
    const sql = planned[PROCUREMENT_RD_REQUEST_ALLOCATIONS];
    const allocation = { ordinal: 1, allocated_milli: 5000 };
    const fetchMock = stubFetch({ data: { statements: [{ sql, values: ['req-1'] }] } });
    const { db, prepared, bound } = fakeDb([allocation]);
    const facts = EmployeeRequestsProcurementOwnedFacts(db as never);
    expect((await facts.readActOnEmployeeRequestAllocations('req-1')).results).toEqual([allocation]);
    expect((await facts.readRequestQuotationSourceAllocated('req-1')).results).toEqual([allocation]);
    expect(prepared).toEqual([sql, sql]);
    expect(bound).toEqual([['req-1'], ['req-1']]);
    const [, init] = fetchMock.mock.calls[0] as unknown as [URL, RequestInit];
    const body = JSON.parse(init.body as string) as { kind: string; values: unknown[] };
    expect(body.kind).toBe('procurement_rd_request_allocations');
    expect(body.values).toEqual(['req-1']);
    expect(JSON.stringify(body)).not.toContain('SELECT');

    const literal = "SELECT l.request_item_ordinal ordinal,COALESCE(sum(l.request_quantity_milli),0) allocated_milli FROM procurement_rfq_lines l JOIN procurement_rfqs r ON r.id=l.rfq_id WHERE r.request_id=? AND r.status<>'cancelled' GROUP BY l.request_item_ordinal";
    for (const rel of [
      '../../../packages/server-procurement/src/adapters/records-employee-requests-owned-facts.ts',
      '../../../packages/server-procurement/src/adapters/purchase-request-progress.ts',
    ]) {
      const source = readFileSync(new URL(rel, import.meta.url), 'utf8');
      expect(source).not.toContain(literal);
      expect(source).toContain('PROCUREMENT_RD_REQUEST_ALLOCATIONS');
    }
  });

  it('plans the request-order link read and retires both TypeScript literals', async () => {
    const sql = planned[PROCUREMENT_RD_REQUEST_ORDER_LINK];
    const link = { request_id: 'req-1' };
    const fetchMock = stubFetch({ data: { statements: [{ sql, values: ['req-1', 'order-1'] }] } });
    const { db, prepared, bound } = fakeDb([link]);
    const facts = EmployeeRequestsProcurementOwnedFacts(db as never);
    expect(await facts.readActOnEmployeeRequestLinked('req-1', 'order-1')).toEqual(link);
    expect(await facts.readPrepareRequestOrderHandoffExistingLink('req-1', 'order-1')).toEqual(link);
    expect(prepared).toEqual([sql, sql]);
    expect(bound).toEqual([['req-1', 'order-1'], ['req-1', 'order-1']]);
    const [, init] = fetchMock.mock.calls[0] as unknown as [URL, RequestInit];
    const body = JSON.parse(init.body as string) as { kind: string; values: unknown[] };
    expect(body.kind).toBe('procurement_rd_request_order_link');
    expect(body.values).toEqual(['req-1', 'order-1']);
    expect(JSON.stringify(body)).not.toContain('SELECT');

    const source = readFileSync(
      new URL('../../../packages/server-procurement/src/adapters/records-employee-requests-owned-facts.ts', import.meta.url),
      'utf8',
    );
    expect(source).not.toContain('SELECT 1 ok FROM procurement_request_orders WHERE request_id=? AND order_id=?');
    expect(source).not.toContain('SELECT request_id FROM procurement_request_orders WHERE request_id=? AND order_id=?');
    expect(source).toContain('PROCUREMENT_RD_REQUEST_ORDER_LINK');
  });

  it('plans the bill-match variance read and retires the supplier-bills SELECT', async () => {
    const sql = planned[PROCUREMENT_RD_BILL_MATCH_VARIANCE];
    const fetchMock = stubFetch({ data: { statements: [{ sql, values: ['bill-1'] }] } });
    const { db, prepared, bound } = fakeDb([{ id: 'line-1' }]);
    const row = await nativeProcurementReadFirst<{ id: string }>(
      PROCUREMENT_RD_BILL_MATCH_VARIANCE, ['bill-1'], db as never,
    );
    expect(row).toEqual({ id: 'line-1' });
    expect(prepared).toEqual([sql]);
    expect(bound).toEqual([['bill-1']]);
    const [, init] = fetchMock.mock.calls[0] as unknown as [URL, RequestInit];
    const body = JSON.parse(init.body as string) as { kind: string; values: unknown[] };
    expect(body.kind).toBe('procurement_rd_bill_match_variance');
    expect(body.values).toEqual(['bill-1']);
    expect(JSON.stringify(body)).not.toContain('SELECT');

    const source = readFileSync(
      new URL('../../../packages/server-procurement/src/adapters/supplier-bills.ts', import.meta.url),
      'utf8',
    );
    expect(source).not.toContain('SELECT id FROM procurement_bill_match_lines WHERE match_id=? AND subtotal_minor<>expected_subtotal_minor LIMIT 1');
    expect(source).toContain('PROCUREMENT_RD_BILL_MATCH_VARIANCE');
  });

  it('plans the bill-match receipt-history read and retires the supplier-bills SELECT', async () => {
    const sql = planned[PROCUREMENT_RD_BILL_MATCH_RECEIPT_HISTORY];
    const fetchMock = stubFetch({ data: { statements: [{ sql, values: ['fulfillment-1'] }] } });
    const { db, prepared, bound } = fakeDb([{ id: 'bill-1' }]);
    const row = await nativeProcurementReadFirst<{ id: string }>(
      PROCUREMENT_RD_BILL_MATCH_RECEIPT_HISTORY, ['fulfillment-1'], db as never,
    );
    expect(row).toEqual({ id: 'bill-1' });
    expect(prepared).toEqual([sql]);
    expect(bound).toEqual([['fulfillment-1']]);
    const [, init] = fetchMock.mock.calls[0] as unknown as [URL, RequestInit];
    const body = JSON.parse(init.body as string) as { kind: string; values: unknown[] };
    expect(body.kind).toBe('procurement_rd_bill_match_receipt_history');
    expect(body.values).toEqual(['fulfillment-1']);
    expect(JSON.stringify(body)).not.toContain('SELECT');

    const source = readFileSync(
      new URL('../../../packages/server-procurement/src/adapters/supplier-bills.ts', import.meta.url),
      'utf8',
    );
    expect(source).not.toContain('SELECT br.match_id id FROM procurement_bill_match_receipts br WHERE br.fulfillment_id=? LIMIT 1');
    expect(source).toContain('PROCUREMENT_RD_BILL_MATCH_RECEIPT_HISTORY');
  });

  it('plans the invoice-policy matching-required read and retires the supplier-bills SELECT', async () => {
    const sql = planned[PROCUREMENT_RD_INVOICE_POLICY_MATCHING_REQUIRED];
    const fetchMock = stubFetch({ data: { statements: [{ sql, values: ['company'] }] } });
    const { db, prepared, bound } = fakeDb([{ '1': 1 }]);
    const row = await nativeProcurementReadFirst<{ 1: number }>(
      PROCUREMENT_RD_INVOICE_POLICY_MATCHING_REQUIRED, ['company'], db as never,
    );
    expect(row).toEqual({ '1': 1 });
    expect(prepared).toEqual([sql]);
    expect(bound).toEqual([['company']]);
    const [, init] = fetchMock.mock.calls[0] as unknown as [URL, RequestInit];
    const body = JSON.parse(init.body as string) as { kind: string; values: unknown[] };
    expect(body.kind).toBe('procurement_rd_invoice_policy_matching_required');
    expect(body.values).toEqual(['company']);
    expect(JSON.stringify(body)).not.toContain('SELECT');

    const source = readFileSync(
      new URL('../../../packages/server-procurement/src/adapters/supplier-bills.ts', import.meta.url),
      'utf8',
    );
    expect(source).not.toContain("SELECT 1 FROM procurement_invoice_policies WHERE company_id=? AND matching_mode='required'");
    expect(source).toContain('PROCUREMENT_RD_INVOICE_POLICY_MATCHING_REQUIRED');
  });

  it('plans both record-purchases existence reads and retires the TypeScript literals', async () => {
    const source = readFileSync(
      new URL('../../../packages/server-procurement/src/adapters/record-purchases.ts', import.meta.url),
      'utf8',
    );

    // The two fixed existence literals are retired from the owner adapter.
    expect(source).not.toContain('SELECT 1 FROM procurement_bill_matches WHERE id=?');
    expect(source).not.toContain('SELECT 1 FROM procurement_rfqs WHERE id=?');
    expect(source).toContain('PROCUREMENT_RD_RECORD_BILL_MATCH_EXISTS');
    expect(source).toContain('PROCUREMENT_RD_RECORD_RFQ_EXISTS');

    // The scoped workspace authorization query stays byte-for-byte intact.
    expect(source).toContain('SELECT 1 FROM ${bill?"procurement_bill_matches":"procurement_rfqs"} r WHERE r.id=? AND ${scope.where}');

    // Both kinds plan in Rust and execute on the caller's open unit with the
    // same single `[id]` bind; no SQL crosses the wire.
    for (const [kind, sql] of [
      [PROCUREMENT_RD_RECORD_BILL_MATCH_EXISTS, planned[PROCUREMENT_RD_RECORD_BILL_MATCH_EXISTS]],
      [PROCUREMENT_RD_RECORD_RFQ_EXISTS, planned[PROCUREMENT_RD_RECORD_RFQ_EXISTS]],
    ] as const) {
      const fetchMock = stubFetch({ data: { statements: [{ sql, values: ['rec-1'] }] } });
      const { db, prepared, bound } = fakeDb([{ '1': 1 }]);
      const row = await nativeProcurementReadFirst<{ 1: number }>(kind, ['rec-1'], db as never);
      expect(row).toEqual({ '1': 1 });
      expect(prepared).toEqual([sql]);
      expect(bound).toEqual([['rec-1']]);
      const [, init] = fetchMock.mock.calls[0] as unknown as [URL, RequestInit];
      const body = JSON.parse(init.body as string) as { kind: string; values: unknown[] };
      expect(body.kind).toBe(kind);
      expect(body.values).toEqual(['rec-1']);
      expect(JSON.stringify(body)).not.toContain('SELECT');
    }
  });

  it('plans the bill-match events read and retires the supplier-bills detail SELECT', async () => {
    const sql = planned[PROCUREMENT_RD_BILL_MATCH_EVENTS];
    const fetchMock = stubFetch({ data: { statements: [{ sql, values: ['bill-1'] }] } });
    const { db, prepared, bound } = fakeDb([{ id: 'event-1', version: 1 }]);
    const result = await nativeProcurementReadAll<Record<string, unknown>>(
      PROCUREMENT_RD_BILL_MATCH_EVENTS, ['bill-1'], db as never,
    );
    expect(result.results).toEqual([{ id: 'event-1', version: 1 }]);
    expect(prepared).toEqual([sql]);
    expect(bound).toEqual([['bill-1']]);
    const [, init] = fetchMock.mock.calls[0] as unknown as [URL, RequestInit];
    const body = JSON.parse(init.body as string) as { kind: string; values: unknown[] };
    expect(body.kind).toBe('procurement_rd_bill_match_events');
    expect(body.values).toEqual(['bill-1']);
    expect(JSON.stringify(body)).not.toContain('SELECT');

    const source = readFileSync(
      new URL('../../../packages/server-procurement/src/adapters/supplier-bills.ts', import.meta.url),
      'utf8',
    );
    expect(source).not.toContain('SELECT * FROM procurement_bill_match_events WHERE match_id=? ORDER BY version DESC');
    expect(source).toContain('PROCUREMENT_RD_BILL_MATCH_EVENTS');
  });

  it('plans both commerce-facts bill-match reads and retires the TypeScript literals', async () => {
    const source = readFileSync(
      new URL('../../../packages/server-procurement/src/adapters/commerce-facts.ts', import.meta.url),
      'utf8',
    );
    expect(source).not.toContain('SELECT l.subtotal_minor,l.tax_minor FROM procurement_bill_match_lines');
    expect(source).not.toContain("SELECT * FROM procurement_bill_matches WHERE invoice_id=? AND company_id=? AND status='posted'");
    expect(source).toContain('PROCUREMENT_RD_BILL_MATCH_LINES_FOR_INVOICE');
    expect(source).toContain('PROCUREMENT_RD_POSTED_BILL_MATCH_FOR_INVOICE');

    for (const [kind, values, rows] of [
      [PROCUREMENT_RD_BILL_MATCH_LINES_FOR_INVOICE, ['inv-1', 'line-1'], [{ subtotal_minor: 1000, tax_minor: 70 }]],
      [PROCUREMENT_RD_POSTED_BILL_MATCH_FOR_INVOICE, ['inv-1', 'company'], [{ id: 'bill-1' }]],
    ] as const) {
      const fetchMock = stubFetch({ data: { statements: [{ sql: planned[kind], values }] } });
      const { db, prepared, bound } = fakeDb([...rows]);
      const facts = CommerceProcurementFacts(db as never);
      const row = kind === PROCUREMENT_RD_BILL_MATCH_LINES_FOR_INVOICE
        ? await facts.readProcurementBillMatchLines1({ invoice_id: 'inv-1', id: 'line-1' })
        : await facts.readProcurementBillMatches1({ invoice_id: 'inv-1', companyId: 'company' });
      expect(row).toEqual(rows[0]);
      expect(prepared).toEqual([planned[kind]]);
      expect(bound).toEqual([[...values]]);
      const [, init] = fetchMock.mock.calls[0] as unknown as [URL, RequestInit];
      const body = JSON.parse(init.body as string) as { kind: string; values: unknown[] };
      expect(body.kind).toBe(kind);
      expect(body.values).toEqual([...values]);
      expect(JSON.stringify(body)).not.toContain('SELECT');
    }
  });

  it('plans both employee-request quotation lookups and retires the TypeScript literals', async () => {
    const ordered = { id: 'rfq-1', order_id: 'order-1' };
    const orderedMock = stubFetch({ data: { statements: [{ sql: planned[PROCUREMENT_RD_EMPLOYEE_REQUEST_ORDERED_QUOTATION], values: ['req-1', 'order-1'] }] } });
    const orderedDb = fakeDb([ordered]);
    const facts = EmployeeRequestsProcurementOwnedFacts(orderedDb.db as never);
    expect(await facts.readActOnEmployeeRequestQuotation('req-1', 'order-1')).toEqual(ordered);
    expect(orderedDb.prepared).toEqual([planned[PROCUREMENT_RD_EMPLOYEE_REQUEST_ORDERED_QUOTATION]]);
    expect(orderedDb.bound).toEqual([['req-1', 'order-1']]);
    const [, orderedInit] = orderedMock.mock.calls[0] as unknown as [URL, RequestInit];
    const orderedBody = JSON.parse(orderedInit.body as string) as { kind: string; values: unknown[] };
    expect(orderedBody.kind).toBe('procurement_rd_employee_request_ordered_quotation');
    expect(orderedBody.values).toEqual(['req-1', 'order-1']);
    expect(JSON.stringify(orderedBody)).not.toContain('SELECT');

    const approved = { id: 'rfq-2' };
    const approvedMock = stubFetch({ data: { statements: [{ sql: planned[PROCUREMENT_RD_HANDOFF_APPROVED_QUOTATION], values: ['req-1', 'rfq-2'] }] } });
    const approvedDb = fakeDb([approved]);
    const handoffFacts = EmployeeRequestsProcurementOwnedFacts(approvedDb.db as never);
    expect(await handoffFacts.readPrepareRequestOrderHandoffQuotation('req-1', 'rfq-2')).toEqual(approved);
    expect(approvedDb.prepared).toEqual([planned[PROCUREMENT_RD_HANDOFF_APPROVED_QUOTATION]]);
    expect(approvedDb.bound).toEqual([['req-1', 'rfq-2']]);
    const [, approvedInit] = approvedMock.mock.calls[0] as unknown as [URL, RequestInit];
    const approvedBody = JSON.parse(approvedInit.body as string) as { kind: string; values: unknown[] };
    expect(approvedBody.kind).toBe('procurement_rd_handoff_approved_quotation');
    expect(approvedBody.values).toEqual(['req-1', 'rfq-2']);
    expect(JSON.stringify(approvedBody)).not.toContain('SELECT');

    const source = readFileSync(
      new URL('../../../packages/server-procurement/src/adapters/records-employee-requests-owned-facts.ts', import.meta.url),
      'utf8',
    );
    expect(source).not.toContain("SELECT id,order_id FROM procurement_rfqs WHERE request_id=? AND order_id=? AND status='ordered'");
    expect(source).not.toContain("SELECT id FROM procurement_rfqs WHERE request_id=? AND id=? AND status='approved'");
    expect(source).toContain('PROCUREMENT_RD_EMPLOYEE_REQUEST_ORDERED_QUOTATION');
    expect(source).toContain('PROCUREMENT_RD_HANDOFF_APPROVED_QUOTATION');
  });

  it('plans the four owner-local read-tail kinds and retires their TypeScript literals', async () => {
    const cases = [
      [PROCUREMENT_RD_PROGRESS_REQUEST_RFQS, ['req-1', 'company'], [{ id: 'rfq-1', status: 'ordered', order_id: 'order-1' }]],
      [PROCUREMENT_RD_RFQ_BY_ORDER, ['order-1', 'company'], [{ id: 'rfq-1' }]],
      [PROCUREMENT_RD_BILL_BY_INVOICE, ['inv-1', 'company'], [{ id: 'bill-1' }]],
      [PROCUREMENT_RD_LATEST_BILL_BY_FULFILLMENT, ['fulfillment-1'], [{ id: 'bill-1', status: 'posted' }]],
    ] as const;
    for (const [kind, values, rows] of cases) {
      const fetchMock = stubFetch({ data: { statements: [{ sql: planned[kind], values }] } });
      const { db, prepared, bound } = fakeDb([...rows]);
      const result = await nativeProcurementReadAll<Record<string, unknown>>(kind, values, db as never);
      expect(result.results).toEqual([...rows]);
      expect(prepared).toEqual([planned[kind]]);
      expect(bound).toEqual([[...values]]);
      const [, init] = fetchMock.mock.calls[0] as unknown as [URL, RequestInit];
      const body = JSON.parse(init.body as string) as { kind: string; values: unknown[] };
      expect(body.kind).toBe(kind);
      expect(body.values).toEqual([...values]);
      expect(JSON.stringify(body)).not.toContain('SELECT');
    }

    const progress = readFileSync(
      new URL('../../../packages/server-procurement/src/adapters/purchase-request-progress.ts', import.meta.url),
      'utf8',
    );
    expect(progress).not.toContain("SELECT id,status,order_id FROM procurement_rfqs WHERE request_id=? AND company_id=? AND status<>'cancelled' ORDER BY updated_at DESC,id DESC");
    expect(progress).toContain('PROCUREMENT_RD_PROGRESS_REQUEST_RFQS');

    const rfqs = readFileSync(
      new URL('../../../packages/server-procurement/src/adapters/rfqs.ts', import.meta.url),
      'utf8',
    );
    expect(rfqs).not.toContain('SELECT id FROM procurement_rfqs WHERE order_id=? AND company_id=?');
    expect(rfqs).toContain('PROCUREMENT_RD_RFQ_BY_ORDER');

    const bills = readFileSync(
      new URL('../../../packages/server-procurement/src/adapters/supplier-bills.ts', import.meta.url),
      'utf8',
    );
    expect(bills).not.toContain('SELECT * FROM procurement_bill_matches WHERE invoice_id=? AND company_id=?');
    expect(bills).not.toContain("SELECT id,status FROM procurement_bill_matches WHERE fulfillment_id=? ORDER BY (status<>'returned') DESC,created_at DESC,id DESC LIMIT 1");
    expect(bills).toContain('PROCUREMENT_RD_BILL_BY_INVOICE');
    expect(bills).toContain('PROCUREMENT_RD_LATEST_BILL_BY_FULFILLMENT');
  });

  it('fails closed when the native transport is not configured', async () => {
    delete process.env[envKey];
    await expect(nativeProcurementReadAll(
      PROCUREMENT_RD_REQUEST_QUOTATION, ['req-1', 'company'], fakeDb().db as never,
    )).rejects.toMatchObject({ status: 503 });
  });
});
