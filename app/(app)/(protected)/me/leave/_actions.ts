'use server';

import 'server-only';
import { revalidatePath } from 'next/cache';
import { redirect } from 'next/navigation';
import { loadActor } from '@/server/guard';
import { submitLeave } from '@/hr/server';

export async function submitMyLeaveAction(formData: FormData): Promise<void> {
  const actor = await loadActor();
  if (!actor) redirect('/login');

  const leaveType = String(formData.get('leaveType') ?? 'sick') as 'sick' | 'annual' | 'personal';
  const startDate = String(formData.get('startDate') ?? '');
  const endDate = String(formData.get('endDate') ?? '');
  const daysRaw = String(formData.get('days') ?? '');
  const days = daysRaw ? parseFloat(daysRaw) : 0;
  const reason = String(formData.get('reason') ?? '').trim() || null;

  if (!startDate || !endDate || days <= 0) {
    throw new Error('Invalid input');
  }

  await submitLeave({
    employeeId: actor.id,
    leaveType,
    startDate,
    endDate,
    days,
    reason,
  });

  revalidatePath('/me/leave');
  redirect('/me/leave');
}
