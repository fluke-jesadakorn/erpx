import 'server-only';
import { PDFDocument, StandardFonts } from 'pdf-lib';
import { query } from '../db';
import { put } from '../slips/storage';
import { recordAttachment } from '../waybill/attachments';
import { buildPoBuffer } from './poPdf';

export type ExpensePaymentMethod = 'cash' | 'credit_card' | 'transfer';

export interface ExpensePaymentPreview {
  waybillId: string;
  expenseId: number;
  amount: number;
  currency: string;
  method: ExpensePaymentMethod;
  payeeType: 'employee' | 'vendor';
  payee: string;
  bankName: string;
  bankBranch: string | null;
  accountNumber: string | null;
  companyName: string;
  companyTaxId: string | null;
  sourceAttachmentKey: string | null;
  ready: boolean;
  blocker: string | null;
}

interface ExpensePaymentPreviewRow {
  waybill_id: string;
  expense_id: number;
  payee_type: 'employee' | 'vendor' | null;
  payment_method: string | null;
  vendor_name: string | null;
  submitter_name: string | null;
  open_amount: string | null;
  currency: string | null;
  bank_name: string | null;
  bank_branch: string | null;
  account_number: string | null;
  account_name: string | null;
  source_attachment_key: string | null;
  company_name: string | null;
  company_tax_id: string | null;
}

interface ExpenseDocumentRow {
  expense_id: number;
  payee_type: 'employee' | 'vendor';
  vendor_name: string | null;
  total_amount: string | null;
  currency: string;
  submitter_name: string;
  transaction_date: Date | null;
}

function safe(value: string | null | undefined): string {
  return (value ?? '').normalize('NFKD').replace(/[^\x20-\x7E\u00A0-\u00FF]/g, '?');
}

export async function loadExpensePaymentPreview(
  waybillId: string,
  q: typeof query = query,
): Promise<ExpensePaymentPreview | null> {
  const res = await q<ExpensePaymentPreviewRow>(
    `SELECT wb.id AS waybill_id, e.id AS expense_id, e.payee_type, e.payment_method,
            e.vendor_name, u.fullname AS submitter_name,
            ap.open_foreign::text AS open_amount,
            COALESCE(ap.currency_code, wb.currency, cfg.functional_currency, 'THB') AS currency,
            bank.bank_name, bank.bank_branch, bank.account_number, bank.account_name,
            bank.file_path AS source_attachment_key,
            cfg.legal_name AS company_name, cfg.tax_id AS company_tax_id
       FROM waybills wb
       JOIN expenses e ON wb.origin = 'expense' AND e.id = wb.origin_id
       LEFT JOIN users u ON u.id = e.submitter_id
       LEFT JOIN finance.company_config cfg ON cfg.id = 1
       LEFT JOIN LATERAL (
         SELECT d.open_foreign, d.currency_code
           FROM finance.ap_documents d
          WHERE d.source_type = 'expense' AND d.source_id = e.id::text
            AND d.status IN ('open', 'partially_paid')
          ORDER BY d.id DESC LIMIT 1
       ) ap ON TRUE
       LEFT JOIN LATERAL (
         SELECT s.bank_name, s.bank_branch, s.account_number, s.account_name, s.file_path
           FROM slips s
          WHERE s.expense_id = e.id AND s.kind = 'book_bank' AND s.status = 'confirmed'
          ORDER BY s.id DESC LIMIT 1
       ) bank ON TRUE
      WHERE wb.id = $1`,
    [waybillId],
  );
  const row = res.rows[0];
  if (!row) return null;
  const method: ExpensePaymentMethod = row.payment_method === 'transfer' || row.payment_method === 'credit_card'
    ? row.payment_method
    : 'cash';
  const payeeType = row.payee_type === 'vendor' ? 'vendor' : 'employee';
  const payee = row.account_name
    || (payeeType === 'vendor' ? row.vendor_name : row.submitter_name)
    || '';
  const bankName = method === 'transfer'
    ? row.bank_name ?? ''
    : method === 'credit_card' ? 'Corporate card' : 'Cash payout';
  const amount = Number(row.open_amount ?? 0);
  const missingBank = method === 'transfer'
    && (!row.source_attachment_key || !row.bank_name || !row.account_number || !row.account_name);
  const blocker = amount <= 0
    ? 'No open approved amount is available for payment'
    : missingBank
      ? 'The submitter must attach and confirm bank details before transfer'
      : !payee
        ? 'Payment recipient is missing'
        : null;
  return {
    waybillId: row.waybill_id,
    expenseId: row.expense_id,
    amount,
    currency: (row.currency ?? 'THB').trim(),
    method,
    payeeType,
    payee,
    bankName,
    bankBranch: row.bank_branch,
    accountNumber: method === 'transfer' ? row.account_number : null,
    companyName: row.company_name ?? 'Folio Company Limited',
    companyTaxId: row.company_tax_id,
    sourceAttachmentKey: row.source_attachment_key,
    ready: blocker === null,
    blocker,
  };
}

export async function buildMockExpensePaymentSlip(args: {
  payment: ExpensePaymentPreview;
  amount: number;
  paymentDate: string;
  reference: string;
  recordedBy: string;
}): Promise<Buffer> {
  const { payment } = args;
  const xml = (value: string | null | undefined) => (value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&apos;');
  const amount = `${args.amount.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })} ${payment.currency}`;
  const method = payment.method.replace('_', ' ').toUpperCase();
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="880" height="560" viewBox="0 0 880 560" font-family="ui-sans-serif,system-ui,-apple-system,Thonburi,'Noto Sans Thai',sans-serif">
  <rect width="880" height="560" fill="#f8fafc"/>
  <rect width="880" height="92" fill="#0f1a2d"/>
  <text x="40" y="44" font-size="22" font-weight="800" fill="#f8fafc" letter-spacing="1.5">SIMULATED PAYMENT SLIP</text>
  <text x="40" y="70" font-size="12" fill="#a8b3c5">Generated from approved Folio payment data</text>
  <rect x="710" y="22" width="130" height="48" rx="24" fill="#18a66f"/>
  <text x="775" y="53" text-anchor="middle" font-size="18" font-weight="800" fill="#ffffff" letter-spacing="2">PAID</text>
  <text x="40" y="128" font-size="11" fill="#64748b" letter-spacing="1">PAID FROM</text>
  <text x="40" y="153" font-size="18" font-weight="700" fill="#0f172a">${xml(payment.companyName)}</text>
  <text x="40" y="174" font-size="11" fill="#64748b">Tax ID ${xml(payment.companyTaxId) || '-'}</text>
  <line x1="40" y1="198" x2="840" y2="198" stroke="#cbd5e1"/>
  <text x="40" y="228" font-size="10" fill="#64748b" letter-spacing="1">WAYBILL</text>
  <text x="40" y="251" font-size="15" font-weight="700" fill="#0f172a">${xml(payment.waybillId)}</text>
  <text x="250" y="228" font-size="10" fill="#64748b" letter-spacing="1">EXPENSE</text>
  <text x="250" y="251" font-size="15" font-weight="700" fill="#0f172a">EXP-${payment.expenseId}</text>
  <text x="460" y="228" font-size="10" fill="#64748b" letter-spacing="1">PAYMENT DATE</text>
  <text x="460" y="251" font-size="15" font-weight="700" fill="#0f172a">${xml(args.paymentDate)}</text>
  <text x="650" y="228" font-size="10" fill="#64748b" letter-spacing="1">METHOD</text>
  <text x="650" y="251" font-size="15" font-weight="700" fill="#0f172a">${xml(method)}</text>
  <rect x="40" y="282" width="800" height="114" rx="10" fill="#ffffff" stroke="#dbe2ea"/>
  <text x="62" y="311" font-size="10" fill="#64748b" letter-spacing="1">PAID TO / ผู้รับเงิน</text>
  <text x="62" y="339" font-size="18" font-weight="700" fill="#0f172a">${xml(payment.payee)}</text>
  <text x="62" y="371" font-size="11" fill="#64748b">${xml(payment.bankName)}${payment.bankBranch ? ` · ${xml(payment.bankBranch)}` : ''}</text>
  <text x="818" y="371" text-anchor="end" font-size="13" font-weight="700" fill="#0f5f9e">${xml(payment.accountNumber) || method}</text>
  <rect x="40" y="420" width="800" height="84" rx="10" fill="#e9fbf3" stroke="#86d8b6"/>
  <text x="62" y="451" font-size="10" font-weight="700" fill="#08754d" letter-spacing="1">AMOUNT PAID / จำนวนเงิน</text>
  <text x="818" y="479" text-anchor="end" font-size="30" font-weight="800" fill="#075f40">${xml(amount)}</text>
  <text x="40" y="533" font-size="10" fill="#9a4b1f">SIMULATION ONLY · No bank integration was called · Reference ${xml(args.reference)} · Recorded by ${xml(args.recordedBy)}</text>
</svg>`;
  return Buffer.from(svg);
}

async function voucher(row: ExpenseDocumentRow, waybillId: string, approvedBy: string) {
  const pdf = await PDFDocument.create();
  const page = pdf.addPage([595, 842]);
  const font = await pdf.embedFont(StandardFonts.Helvetica);
  const bold = await pdf.embedFont(StandardFonts.HelveticaBold);
  page.drawText('EXPENSE REIMBURSEMENT VOUCHER', { x: 50, y: 790, size: 18, font: bold });
  const fields = [
    `Waybill: ${waybillId}`,
    `Employee: ${safe(row.submitter_name)}`,
    `Expense date: ${row.transaction_date ? new Date(row.transaction_date).toISOString().slice(0, 10) : '-'}`,
    `Approved amount: ${Number(row.total_amount ?? 0).toFixed(2)} ${row.currency}`,
    `Approved by: ${safe(approvedBy)}`,
    `Generated: ${new Date().toISOString()}`,
  ];
  fields.forEach((field, index) => page.drawText(field, { x: 50, y: 745 - index * 24, size: 11, font }));
  return Buffer.from(await pdf.save());
}

export async function ensureExpensePaymentDocument(args: {
  waybillId: string;
  actorId: number;
  actorRole: string;
  actorName: string;
}) {
  const existing = await query<{ id: number; storage_key: string; kind: 'po_doc' | 'expense_voucher' }>(
    `SELECT id, storage_key, kind FROM waybill_attachments
      WHERE waybill_id = $1 AND kind IN ('po_doc', 'expense_voucher')
      ORDER BY id DESC LIMIT 1`,
    [args.waybillId],
  );
  if (existing.rows[0]) return existing.rows[0];
  const res = await query<ExpenseDocumentRow>(
    `SELECT e.id AS expense_id, e.payee_type, e.vendor_name, e.total_amount::text,
            wb.currency, u.fullname AS submitter_name, e.transaction_date
       FROM waybills wb
       JOIN expenses e ON wb.origin = 'expense' AND e.id = wb.origin_id
       JOIN users u ON u.id = e.submitter_id
      WHERE wb.id = $1`,
    [args.waybillId],
  );
  const row = res.rows[0];
  if (!row) throw new Error('Expense waybill not found');
  const vendor = row.payee_type === 'vendor';
  const buffer = vendor
    ? await buildPoBuffer({
        waybillId: args.waybillId,
        poNumber: `PO-${args.waybillId}`,
        vendorName: row.vendor_name ?? '',
        totalAmount: Number(row.total_amount ?? 0),
        currency: row.currency,
        lineItems: [],
        requestedBy: row.submitter_name,
        approvedBy: args.actorName,
        approvedAt: new Date(),
      })
    : await voucher(row, args.waybillId, args.actorName);
  const kind = vendor ? 'po_doc' : 'expense_voucher';
  const key = vendor ? `po/${args.waybillId}.pdf` : `expense-voucher/${args.waybillId}.pdf`;
  const filename = vendor ? `${args.waybillId}-PO.pdf` : `${args.waybillId}-voucher.pdf`;
  await put(key, buffer, 'application/pdf');
  return recordAttachment({
    waybillId: args.waybillId,
    stageKey: 'payment',
    kind,
    storageKey: key,
    filename,
    contentType: 'application/pdf',
    byteSize: buffer.length,
    actorId: args.actorId,
    actorRole: args.actorRole,
    caption: vendor ? 'Vendor purchase order' : 'Employee reimbursement voucher',
  });
}
