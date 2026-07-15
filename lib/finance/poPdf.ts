// lib/finance/poPdf.ts — PO PDF generation + MinIO storage.
//
// Generated at the accounting_verification → accounting_authorization
// transition. Key: po/<waybillId>.pdf in bucket folio-storage.

import 'server-only';
import { PDFDocument, StandardFonts } from 'pdf-lib';
import { query } from '../db';
import { put, presignedGetUrl } from '../slips/storage';

export interface PoLineItem {
  description: string;
  quantity: number;
  unitPrice: number;
  amount: number;
}

export interface PoData {
  waybillId: string;
  poNumber: string;
  vendorName: string;
  totalAmount: number | null;
  currency: string;
  lineItems: PoLineItem[];
  requestedBy: string;
  approvedBy: string;
  approvedAt: Date;
}

export function poStorageKey(waybillId: string): string {
  return `po/${waybillId}.pdf`;
}

function safe(s: string | null | undefined): string {
  if (s == null) return '';
  return s
    .normalize('NFKD')
    .replace(/[^\x09\x0A\x0D\x20-\x7E\u00A0-\u00FF]/g, '?');
}

function fmtThb(n: number | null | undefined, currency = 'THB'): string {
  if (n == null) return '0.00';
  return `${n.toFixed(2)} ${currency}`;
}

async function drawHeader(page: import('pdf-lib').PDFPage, font: import('pdf-lib').PDFFont, bold: import('pdf-lib').PDFFont, text: string, y: number): Promise<number> {
  page.drawText(text, { x: 50, y, size: 11, font: bold });
  return y - 14;
}

export async function buildPoBuffer(po: PoData): Promise<Buffer> {
  const pdf = await PDFDocument.create();
  const page = pdf.addPage([595, 842]);
  const font = await pdf.embedFont(StandardFonts.Helvetica);
  const bold = await pdf.embedFont(StandardFonts.HelveticaBold);

  let y = 800;
  page.drawText('PURCHASE ORDER', { x: 50, y, size: 18, font: bold });
  y -= 30;

  page.drawText(`PO Number: ${safe(po.poNumber)}`, { x: 50, y, size: 11, font: bold });
  y -= 14;
  page.drawText(`Waybill: ${safe(po.waybillId)}`, { x: 50, y, size: 11, font });
  y -= 14;
  page.drawText(`Date: ${po.approvedAt.toISOString().slice(0, 10)}`, { x: 50, y, size: 11, font });
  y -= 24;

  y = await drawHeader(page, font, bold, 'VENDOR', y);
  page.drawText(safe(po.vendorName), { x: 50, y, size: 11, font });
  y -= 24;

  y = await drawHeader(page, font, bold, 'REQUESTED BY', y);
  page.drawText(safe(po.requestedBy), { x: 50, y, size: 11, font });
  y -= 24;

  y = await drawHeader(page, font, bold, 'APPROVED BY', y);
  page.drawText(safe(po.approvedBy), { x: 50, y, size: 11, font });
  y -= 24;

  y = await drawHeader(page, font, bold, 'LINE ITEMS', y);
  page.drawText('Description', { x: 50, y, size: 10, font: bold });
  page.drawText('Qty', { x: 360, y, size: 10, font: bold });
  page.drawText('Unit', { x: 410, y, size: 10, font: bold });
  page.drawText('Amount', { x: 480, y, size: 10, font: bold });
  y -= 14;
  page.drawLine({ start: { x: 50, y }, end: { x: 545, y }, thickness: 0.5 });
  y -= 14;

  if (po.lineItems.length === 0) {
    page.drawText('(see waybill details)', { x: 50, y, size: 10, font });
    y -= 14;
  } else {
    for (const li of po.lineItems) {
      if (y < 80) break;
      page.drawText(safe(li.description).slice(0, 50), { x: 50, y, size: 10, font });
      page.drawText(String(li.quantity), { x: 360, y, size: 10, font });
      page.drawText(li.unitPrice.toFixed(2), { x: 410, y, size: 10, font });
      page.drawText(li.amount.toFixed(2), { x: 480, y, size: 10, font });
      y -= 14;
    }
  }

  y -= 14;
  page.drawLine({ start: { x: 50, y }, end: { x: 545, y }, thickness: 0.5 });
  y -= 14;
  page.drawText('TOTAL', { x: 50, y, size: 11, font: bold });
  page.drawText(fmtThb(po.totalAmount, po.currency), { x: 480, y, size: 11, font: bold });

  return Buffer.from(await pdf.save());
}

interface PoRow {
  po_number: string;
  vendor_name: string | null;
  total_amount: string | null;
  currency: string;
  requester_name: string | null;
}

export async function loadPoRowFor(waybillId: string): Promise<PoRow | null> {
  const r = await query<PoRow>(
    `SELECT po.po_number, e.vendor_name, po.total_amount::text AS total_amount,
            po.currency, u.fullname AS requester_name
       FROM waybills wb
       LEFT JOIN expenses e ON e.id = wb.origin_id AND wb.origin = 'expense'
       LEFT JOIN purchase_orders po ON po.id = wb.origin_id AND wb.origin = 'po'
       LEFT JOIN users u
         ON u.id = CASE WHEN wb.origin = 'expense' THEN e.submitter_id ELSE po.issued_by END
      WHERE wb.id = $1
      LIMIT 1`,
    [waybillId],
  );
  const row = r.rows[0];
  return row && row.po_number ? row : null;
}

export async function generatePoPdf(
  waybillId: string,
  approvedByName: string,
): Promise<string> {
  const row = await loadPoRowFor(waybillId);
  if (!row) throw new Error(`PO not generated yet for ${waybillId}`);
  const total = row.total_amount != null ? parseFloat(row.total_amount) : null;
  const data: PoData = {
    waybillId,
    poNumber: row.po_number,
    vendorName: row.vendor_name ?? '',
    totalAmount: total,
    currency: row.currency ?? 'THB',
    lineItems: [],
    requestedBy: row.requester_name ?? '',
    approvedBy: approvedByName,
    approvedAt: new Date(),
  };
  const buffer = await buildPoBuffer(data);
  const key = poStorageKey(waybillId);
  await put(key, buffer, 'application/pdf');
  return key;
}

export async function ensurePoPdf(waybillId: string, approvedByName: string): Promise<string> {
  const key = poStorageKey(waybillId);
  try {
    await presignedGetUrl(key, 60);
    return key;
  } catch {
    return await generatePoPdf(waybillId, approvedByName);
  }
}
