'use server';

import { revalidatePath } from 'next/cache';
import { z } from 'zod';
import { query } from '@/db';
import { hasPermission } from '@folio-lib/perm/server';
import { PERM } from '@folio-lib/perm/taxonomy';
import { requireActor } from '@/server/guard';
import { generateReviewHint } from '@/waybill/reviewHint';

const ReviewHintForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
  stage: z.enum(['hod', 'am']),
  lang: z.enum(['en', 'th', 'de']).default('en'),
});

export async function loadReviewHints(waybillId: string): Promise<{
  hod: string | null;
  am: string | null;
  generatedAt: { hod: string | null; am: string | null };
}> {
  const res = await query<{
    stage: 'hod' | 'am';
    hint: string;
    generated_at: Date;
  }>(
    `SELECT stage, hint, generated_at
       FROM folio.waybill_reviews
      WHERE waybill_id = $1
        AND stage IN ('hod', 'am')`,
    [waybillId],
  );
  const out: {
    hod: string | null;
    am: string | null;
    generatedAt: { hod: string | null; am: string | null };
  } = {
    hod: null,
    am: null,
    generatedAt: { hod: null, am: null },
  };
  for (const row of res.rows) {
    out[row.stage] = row.hint;
    out.generatedAt[row.stage] = row.generated_at.toISOString();
  }
  return out;
}

export async function generateReviewHintAction(
  prevState: { waybillId: string; stage: 'hod' | 'am' } | null,
  formData: FormData,
): Promise<{ ok: boolean; hint?: string; error?: string }> {
  void prevState;
  let actor;
  try {
    actor = await requireActor();
  } catch {
    return { ok: false, error: 'unauthorized' };
  }

  const parsed = ReviewHintForm.safeParse({
    waybillId: String(formData.get('waybillId') ?? ''),
    stage: String(formData.get('stage') ?? ''),
    lang: String(formData.get('lang') ?? 'en'),
  });
  if (!parsed.success) return { ok: false, error: 'invalid input' };

  const allowed = hasPermission(actor, PERM.finance.expense.view_own)
    || hasPermission(actor, PERM.finance.expense.view_all)
    || hasPermission(actor, PERM.admin.system.bypass);
  if (!allowed) return { ok: false, error: 'forbidden' };

  const { waybillId, stage, lang } = parsed.data;
  const hint = await generateReviewHint({ waybillId, stage, lang }).catch(() => null);
  revalidatePath(`/waybill/${waybillId}`);
  return hint
    ? { ok: true, hint: hint.hint }
    : { ok: false, error: 'AI hint unavailable' };
}
