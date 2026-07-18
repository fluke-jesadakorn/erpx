import 'server-only';
import { PDFDocument, StandardFonts } from 'pdf-lib';
import { query } from '../db';
import { put } from '../slips/storage';
import { recordAttachment } from '../waybill/attachments';
import { buildPoBuffer } from './poPdf';

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
