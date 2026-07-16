import 'server-only';
import { aiInvoke } from '@folio-lib/ai/router';
import { query } from '../db';
import { loadWaybill } from './queries';
import { STAGE_TO_ROLE } from '../perm/stages';

const IDLE_HOURS_THRESHOLD = 8;
const MAX_NUDGES_PER_RUN = 20;

export interface NudgeCandidate {
  waybill_id: string;
  stage: string;
  approver_user_id: number;
  idle_hours: number;
  hint: string;
}

export async function findStaleApproverQueues(): Promise<Array<{ waybill_id: string; stage: string; approver_user_id: number; idle_hours: number }>> {
  const r = await query<{ waybill_id: string; current_stage: string; current_owner_user_id: number | null; idle_hours: string | number }>(
    `SELECT w.id AS waybill_id, w.current_stage, w.current_owner_user_id,
            EXTRACT(EPOCH FROM (now() - w.updated_at)) / 3600.0 AS idle_hours
       FROM folio.waybills w
      WHERE w.status = 'open'
        AND w.current_owner_user_id IS NOT NULL
        AND w.updated_at < now() - INTERVAL '8 hours'
   ORDER BY w.updated_at ASC
      LIMIT $1`,
    [MAX_NUDGES_PER_RUN]
  );
  return r.rows
    .filter(row => row.current_owner_user_id != null)
    .map(row => ({
      waybill_id: row.waybill_id,
      stage: row.current_stage,
      approver_user_id: row.current_owner_user_id!,
      idle_hours: Number(row.idle_hours),
    }));
}

export async function generateNudgeHint(waybillId: string, stage: string, lang: 'en' | 'th' | 'de' = 'en'): Promise<string | null> {
  const wb = await loadWaybill(waybillId);
  if (!wb) return null;
  const langLine = lang === 'th'
    ? 'ตอบเป็นภาษาไทย'
    : lang === 'de'
      ? 'Antworten Sie auf Deutsch'
      : 'Reply in English';

  const r = await aiInvoke('hod:approve', 'chat', {
    systemPrompt: `You are a Thai assistant nudging an approver about a waybill that has been idle. ${langLine}. One sentence, ≤ 25 words. Be specific with vendor + amount. No markdown.`,
    text: JSON.stringify({
      waybillId,
      stage,
      vendor: wb.vendor_name,
      amount: wb.total_amount,
      currency: wb.currency,
      submittedAt: wb.created_at,
      idleHours: Math.round((Date.now() - new Date(wb.updated_at).getTime()) / 3_600_000),
    }, null, 0),
    temperature: 0.2,
    maxTokens: 200,
  });
  return r.ok && r.text ? r.text : null;
}

export async function persistNudge(args: { approverUserId: number; waybillId: string; stage: string; hint: string }): Promise<void> {
  await query(
    `INSERT INTO folio.approver_nudges (approver_user_id, waybill_id, stage, hint)
     VALUES ($1, $2, $3, $4)
     ON CONFLICT (approver_user_id, waybill_id, stage)
     DO UPDATE SET hint = EXCLUDED.hint, sent_at = now()`,
    [args.approverUserId, args.waybillId, args.stage, args.hint]
  );
}

export async function runApproverNudge(lang: 'en' | 'th' | 'de' = 'en'): Promise<NudgeCandidate[]> {
  const candidates = await findStaleApproverQueues();
  const sent: NudgeCandidate[] = [];
  for (const c of candidates) {
    const hint = await generateNudgeHint(c.waybill_id, c.stage, lang);
    if (!hint) continue;
    await persistNudge({ approverUserId: c.approver_user_id, waybillId: c.waybill_id, stage: c.stage, hint });
    sent.push({ ...c, hint });
  }
  return sent;
}

export async function listRecentNudgesForUser(userId: number, limit = 10): Promise<Array<{ waybill_id: string; stage: string; hint: string; sent_at: string }>> {
  const r = await query<{ waybill_id: string; stage: string; hint: string; sent_at: Date | string }>(
    `SELECT waybill_id, stage, hint, sent_at
       FROM folio.approver_nudges
      WHERE approver_user_id = $1
   ORDER BY sent_at DESC
      LIMIT $2`,
    [userId, limit]
  );
  return r.rows.map(row => ({
    waybill_id: row.waybill_id,
    stage: row.stage,
    hint: row.hint,
    sent_at: typeof row.sent_at === 'string' ? row.sent_at : new Date(row.sent_at).toISOString(),
  }));
}