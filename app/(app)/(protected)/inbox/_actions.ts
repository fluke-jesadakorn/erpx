'use server';

import 'server-only';
import { revalidatePath } from 'next/cache';
import { z } from 'zod';
import { query } from '@/db';
import { loadActor } from '@/server/guard';
import { removeWatcher } from '@/waybill/watchers';

const WatcherRef = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
  stageKey: z.string().min(1).max(64),
});

export interface InboxItemActionResult {
  ok: boolean;
  error?: string;
}

function readRef(formData: FormData) {
  return WatcherRef.safeParse({
    waybillId: String(formData.get('waybillId') ?? ''),
    stageKey: String(formData.get('stageKey') ?? ''),
  });
}

export async function markInboxItemReadAction(formData: FormData): Promise<InboxItemActionResult> {
  const parsed = readRef(formData);
  if (!parsed.success) {
    return { ok: false, error: parsed.error.issues[0]?.message ?? 'invalid input' };
  }

  const actor = await loadActor();
  if (!actor) return { ok: false, error: 'unauthenticated' };

  await query(
    `UPDATE waybill_watchers
        SET notified_at = COALESCE(notified_at, now())
      WHERE waybill_id = $1
        AND stage_key  = $2
        AND user_id    = $3`,
    [parsed.data.waybillId, parsed.data.stageKey, actor.id],
  );

  revalidatePath('/inbox');
  revalidatePath(`/waybill/${parsed.data.waybillId}`);
  return { ok: true };
}

export async function dismissInboxItemAction(formData: FormData): Promise<void> {
  const parsed = readRef(formData);
  if (!parsed.success) return;

  const actor = await loadActor();
  if (!actor) return;

  await removeWatcher({
    waybillId: parsed.data.waybillId,
    stageKey: parsed.data.stageKey,
    userId: actor.id,
  });

  revalidatePath('/inbox');
  revalidatePath(`/waybill/${parsed.data.waybillId}`);
}
