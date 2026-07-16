import 'server-only';
import { aiInvoke } from '@folio-lib/ai/router';
import { query } from '../db';
import { loadWaybill, domainOf } from './queries';

export type ReviewStage = 'hod' | 'am';

export interface ReviewHint {
  waybillId: string;
  stage: ReviewStage;
  hint: string;
  generatedAt: string;
}

interface ExpenseContext {
  vendor_name: string | null;
  total_amount: string | null;
  transaction_date: Date | null;
  status: string;
  is_corrupted: boolean | null;
  ocr_confidence: number | null;
  rejection_reason: string | null;
}

interface RecentExpense {
  vendor_name: string | null;
  total_amount: string | null;
  created_at: Date;
}

export async function generateReviewHint(args: {
  waybillId: string;
  stage: ReviewStage;
  lang?: 'en' | 'th' | 'de';
}): Promise<ReviewHint | null> {
  const lang = args.lang ?? 'en';
  const wb = await loadWaybill(args.waybillId);
  if (!wb) return null;

  const histRes = await query<ExpenseContext>(
    `SELECT e.vendor_name, e.total_amount::text, e.transaction_date, e.status, e.is_corrupted,
            (SELECT s.ocr_confidence FROM slips s WHERE s.expense_id = e.id ORDER BY s.id DESC LIMIT 1) AS ocr_confidence,
            e.rejection_reason
       FROM expenses e
      WHERE e.id = $1`,
    [wb.origin_id]
  );
  const exp = histRes.rows[0];

  const recentRes = await query<RecentExpense>(
    `SELECT e.vendor_name, e.total_amount::text, e.created_at
       FROM expenses e
      WHERE e.submitter_id = $1
        AND e.created_at > now() - INTERVAL '90 days'
   ORDER BY e.created_at DESC
      LIMIT 20`,
    [wb.submitter_id ?? 0]
  );
  const sameVendorCount = exp?.vendor_name
    ? recentRes.rows.filter(r => r.vendor_name === exp.vendor_name).length
    : 0;

  const sectionKey = args.stage === 'hod' ? 'hod:approve' : 'am:review';
  const sys = args.stage === 'hod'
    ? `You are a Thai head-of-department. In 2-3 short sentences, tell the approver what to look at first on this expense submission. Mention vendor frequency, OCR confidence (if low), and any policy concerns. Be specific. No bullets. ${lang === 'th' ? 'ตอบเป็นภาษาไทย' : lang === 'de' ? 'Antworten Sie auf Deutsch' : 'Reply in English.'}`
    : `You are a Thai accounting reviewer. In 2-3 short sentences, tell the accounting reviewer the highest-risk item to verify on this expense/PR/PO. Cover tax math, OCR confidence, line-item COA mapping, vendor blacklist, and policy compliance. Be specific. No bullets. ${lang === 'th' ? 'ตอบเป็นภาษาไทย' : lang === 'de' ? 'Antworten Sie auf Deutsch' : 'Reply in English.'}`;

  const r = await aiInvoke(sectionKey, 'chat', {
    systemPrompt: sys,
    text: JSON.stringify({
      waybillId: wb.id,
      origin: wb.origin,
      domain: domainOf(wb),
      currentStage: wb.current_stage,
      totalAmount: wb.total_amount,
      currency: wb.currency,
      vendor: exp?.vendor_name,
      transactionDate: exp?.transaction_date,
      ocrConfidence: exp?.ocr_confidence,
      rejectionReason: exp?.rejection_reason,
      submitterRecentCount: recentRes.rows.length,
      submitterSameVendorCount: sameVendorCount,
    }, null, 0),
    temperature: 0.2,
    maxTokens: 500,
  });

  if (!r.ok || !r.text) return null;

  const hint: ReviewHint = {
    waybillId: wb.id,
    stage: args.stage,
    hint: r.text,
    generatedAt: new Date().toISOString(),
  };

  await query(
    `INSERT INTO waybill_reviews (waybill_id, stage, hint, generated_at)
     VALUES ($1, $2, $3, $4)
     ON CONFLICT (waybill_id, stage) DO UPDATE
       SET hint = EXCLUDED.hint, generated_at = EXCLUDED.generated_at`,
    [wb.id, args.stage, r.text, hint.generatedAt]
  ).catch(() => { /* best effort */ });

  return hint;
}