'use server';

import 'server-only';
import { revalidatePath } from 'next/cache';
import { redirect } from 'next/navigation';
import { z } from 'zod';
import {
  approveLeaveWaybill,
  rejectLeaveWaybill,
} from '@/hr/waybill';
import { loadActor, type ActorWithScope } from '@/server/guard';
import { matchPerm } from '@/perm';

const ApproveForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
});

const RejectForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
  reason: z.string().min(5).max(2000),
});

async function requireActor(): Promise<ActorWithScope> {
  const actor = await loadActor();
  if (!actor) redirect('/login');
  return actor;
}

export async function approveLeaveAction(formData: FormData): Promise<void> {
  const parsed = ApproveForm.parse({
    waybillId: String(formData.get('waybillId') ?? ''),
  });

  const actor = await requireActor();

  await approveLeaveWaybill(parsed.waybillId, {
    id: actor.id,
    role_name: actor.role_name,
    permissions: actor.permissions,
  });

  revalidatePath(`/hr/leave/${parsed.waybillId}`);
  revalidatePath('/hr/leave');
  redirect(`/hr/leave/${parsed.waybillId}`);
}

export async function rejectLeaveAction(formData: FormData): Promise<void> {
  const parsed = RejectForm.parse({
    waybillId: String(formData.get('waybillId') ?? ''),
    reason: String(formData.get('reason') ?? '').trim(),
  });

  const actor = await requireActor();

  if (
    !matchPerm(actor.permissions, `stage:hr_review:act::allow`)
    && !matchPerm(actor.permissions, `stage:hr_authorization:act::allow`)
    && !matchPerm(actor.permissions, 'admin:system:bypass::allow')
  ) {
    throw new Error('forbidden');
  }

  await rejectLeaveWaybill(parsed.waybillId, {
    id: actor.id,
    role_name: actor.role_name,
    permissions: actor.permissions,
  }, parsed.reason);

  revalidatePath(`/hr/leave/${parsed.waybillId}`);
  revalidatePath('/hr/leave');
  redirect(`/hr/leave/${parsed.waybillId}`);
}