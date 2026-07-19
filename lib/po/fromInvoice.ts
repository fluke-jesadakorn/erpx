import 'server-only';
import { query, withTransaction } from '../db';
import { put, publicUrlFor } from '../slips/storage';
import { runOcrPipeline, confidenceScore } from '../slips/ocrPipeline';
import { aiInvoke } from '../ai/router';
import { searchCustomers } from '../customer/queries';

export interface PoInvoiceDraft {
  po_invoice_id: number;
  vendor_name: string | null;
  vendor_id: number | null;
  invoice_no: string | null;
  invoice_date: string | null;
  due_date: string | null;
  subtotal: number | null;
  vat_amount: number | null;
  total_amount: number | null;
  currency: string | null;
  items: Array<{ description: string; qty: number; unit_price: number; amount: number }>;
  draft_pr_id: number | null;
  draft_po_id: number | null;
  pr_number: string | null;
  po_number: string | null;
  isCorrupted: boolean;
  confidence: number;
}

const ACCEPTED_MIME = new Set(['application/pdf', 'image/png', 'image/jpeg', 'image/jpg', 'image/webp']);

function normalizeMime(raw: string): string {
  const m = (raw || '').toLowerCase();
  if (m === 'image/jpg') return 'image/jpeg';
  return m;
}

function round2(n: number): number {
  return Math.round(n * 100) / 100;
}

async function runExtractionChat(parsed: Record<string, unknown>, actorId?: number): Promise<{ invoiceNo?: string; dueDate?: string; items?: unknown[] } | null> {
  const r = await aiInvoke('staff:submit', 'chat', {
    systemPrompt: `You complete missing fields in a supplier invoice JSON. Output only JSON {"invoiceNo":"...","dueDate":"YYYY-MM-DD","items":[...]}. If a field is already present, keep it. If uncertain, return empty string. Keep response under 100 words.`,
    text: JSON.stringify(parsed),
    temperature: 0.1,
    maxTokens: 400,
  }, { actorId });
  if (!r.ok || !r.text) return null;
  try {
    return JSON.parse(r.text.match(/\{[\s\S]*\}/)?.[0] ?? '{}');
  } catch {
    return null;
  }
}

export async function ingestPoInvoice(args: {
  buffer: Buffer;
  fileName: string;
  mime: string;
  uploadedBy: number;
}): Promise<PoInvoiceDraft> {
  const mime = normalizeMime(args.mime);
  if (!ACCEPTED_MIME.has(mime)) {
    throw new Error(`Unsupported MIME type: ${args.mime}`);
  }

  const key = `po_invoices/${Date.now()}-${args.fileName}`;
  await put(key, args.buffer, mime);

  const ins = await query<{ id: number }>(
    `INSERT INTO folio.po_invoices (file_path, mime_type, file_size, status, uploaded_by)
     VALUES ($1, $2, $3, 'pending', $4) RETURNING id`,
    [key, mime, args.buffer.length, args.uploadedBy]
  );
  const poInvoiceId: number = Number(ins.rows[0].id);

  let ocr;
  try {
    ocr = await runOcrPipeline(args.buffer, mime, { kind: 'po_invoice', actorId: args.uploadedBy });
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e);
    await query(
      `UPDATE folio.po_invoices SET status = 'failed', error = $2, updated_at = now() WHERE id = $1`,
      [poInvoiceId, msg.slice(0, 2000)]
    );
    throw e;
  }

  const parsed = ocr.parsed as Record<string, unknown>;
  const ocrConfidence = confidenceScore(parsed, ocr.validation as any);

  const chatFill = await runExtractionChat(parsed, args.uploadedBy).catch(() => null);
  if (chatFill && typeof chatFill === 'object') {
    if (!parsed.invoiceNo && typeof chatFill.invoiceNo === 'string') parsed.invoiceNo = chatFill.invoiceNo;
    if (!parsed.dueDate && typeof chatFill.dueDate === 'string') parsed.dueDate = chatFill.dueDate;
    if ((!Array.isArray(parsed.items) || (parsed.items as unknown[]).length === 0) && Array.isArray(chatFill.items)) {
      parsed.items = chatFill.items;
    }
  }

  const vendorName = typeof parsed.vendorName === 'string' ? parsed.vendorName.trim() : '';
  const matches = vendorName ? await searchCustomers(vendorName, 3).catch(() => []) : [];
  const matched = matches[0] ?? null;

  const fiscalYear = new Date().getFullYear();
  const subtotal = Number(parsed.subtotal) || 0;
  const vat = Number(parsed.vatAmount) || 0;
  const total = Number(parsed.totalAmount) || subtotal + vat;
  const currency = (parsed.currency as string) || 'THB';

  const result = await withTransaction(async (client) => {
    const prIns = await client<{ id: number; pr_number: string }>(
      `INSERT INTO folio.purchase_requisitions
         (requester_id, vendor_name, status, total_estimate, currency, justification, vendor_country)
       VALUES ($1, $2, 'submission', $3, $4, $5, 'TH')
       RETURNING id, pr_number`,
      [args.uploadedBy, vendorName || 'UNKNOWN', round2(total), currency, `auto-drafted from PO invoice ${typeof parsed.invoiceNo === 'string' ? parsed.invoiceNo : 'N/A'}`]
    );
    const prId = Number(prIns.rows[0].id);
    const prNumber = prIns.rows[0].pr_number;

    const items = Array.isArray(parsed.items) ? parsed.items : [];
    for (const it of items) {
      const any = it as Record<string, unknown>;
      const desc = typeof any.description === 'string' ? any.description : '';
      const qty = Number(any.qty) || 1;
      const unitPrice = Number(any.unitPrice) || Number(any.amount) || 0;
      if (!desc) continue;
      await client(
        `INSERT INTO folio.pr_items (pr_id, description, qty, unit_price)
         VALUES ($1, $2, $3, $4)`,
        [prId, desc, qty, unitPrice]
      );
    }

    const poNumRow = await client<{ po_number: string }>(
      `SELECT next_purchase_order_number($1) AS po_number`,
      [fiscalYear]
    );
    const poNumber = poNumRow.rows[0].po_number;

    const poIns = await client<{ id: number }>(
      `INSERT INTO folio.purchase_orders (pr_id, po_number, vendor_name, total_amount, currency, status, issued_at, issued_by)
       VALUES ($1, $2, $3, $4, $5, 'submission', now(), $6)
       RETURNING id`,
      [prId, poNumber, vendorName || 'UNKNOWN', round2(total), currency, args.uploadedBy]
    );
    const poId = Number(poIns.rows[0].id);

    await client(
      `UPDATE folio.po_invoices
          SET vendor_name = $2,
              vendor_id = $3,
              invoice_no = $4,
              invoice_date = $5,
              due_date = $6,
              subtotal = $7,
              vat_amount = $8,
              total_amount = $9,
              currency = $10,
              extracted = $11,
              draft_pr_id = $12,
              draft_po_id = $13,
              status = 'drafted',
              updated_at = now()
        WHERE id = $1`,
      [
        poInvoiceId,
        vendorName || null,
        matched?.id ?? null,
        typeof parsed.invoiceNo === 'string' ? parsed.invoiceNo : null,
        typeof parsed.invoiceDate === 'string' ? parsed.invoiceDate : null,
        typeof parsed.dueDate === 'string' ? parsed.dueDate : null,
        round2(Number(parsed.subtotal) || 0),
        round2(Number(parsed.vatAmount) || 0),
        round2(total),
        currency,
        JSON.stringify(parsed),
        prId,
        poId,
      ]
    );

    return { prId, prNumber, poId, poNumber };
  });

  const draftItems: PoInvoiceDraft['items'] = (Array.isArray(parsed.items) ? parsed.items : []).map((it: unknown) => {
    const any = (it ?? {}) as Record<string, unknown>;
    const qty = Number(any.qty ?? 1);
    const unitPrice = Number(any.unitPrice ?? any.amount ?? 0);
    const amount = Number(any.amount ?? qty * unitPrice);
    return {
      description: String(any.description ?? ''),
      qty,
      unit_price: unitPrice,
      amount,
    };
  });

  return {
    po_invoice_id: poInvoiceId,
    vendor_name: vendorName || null,
    vendor_id: matched?.id ?? null,
    invoice_no: typeof parsed.invoiceNo === 'string' ? parsed.invoiceNo : null,
    invoice_date: typeof parsed.invoiceDate === 'string' ? parsed.invoiceDate : null,
    due_date: typeof parsed.dueDate === 'string' ? parsed.dueDate : null,
    subtotal: round2(Number(parsed.subtotal) || 0),
    vat_amount: round2(Number(parsed.vatAmount) || 0),
    total_amount: round2(total),
    currency,
    items: draftItems,
    draft_pr_id: result.prId,
    draft_po_id: result.poId,
    pr_number: result.prNumber,
    po_number: result.poNumber,
    isCorrupted: parsed.isCorrupted === true,
    confidence: ocrConfidence,
  };
}

export function publicUrlForPoInvoice(key: string): string {
  return publicUrlFor(key);
}
