'use server';
import { z } from 'zod';
import { revalidatePath } from 'next/cache';
import { loadActor } from '@/server/guard';
import { matchPerm } from '@/perm';
import { createCustomer as serverCreate, updateCustomer as serverUpdate, blacklistCustomer as serverBlacklist } from '@/customer/queries';

const CreateForm = z.object({
  code: z.string().min(2).max(40).regex(/^[A-Z0-9_-]+$/),
  name: z.string().min(2).max(150),
  name_th: z.string().max(150).optional().nullable(),
  tax_id: z.string().max(20).optional().nullable(),
  billing_address: z.string().max(500).optional().nullable(),
  shipping_address: z.string().max(500).optional().nullable(),
  contact_name: z.string().max(150).optional().nullable(),
  contact_email: z.string().email().optional().nullable(),
  contact_phone: z.string().max(40).optional().nullable(),
  credit_limit_thb: z.coerce.number().min(0).default(0),
  payment_terms: z.string().max(40).default('Net 30'),
});

const UpdateForm = z.object({
  id: z.coerce.number().int().positive(),
  name: z.string().min(2).max(150).optional(),
  name_th: z.string().max(150).optional().nullable(),
  tax_id: z.string().max(20).optional().nullable(),
  billing_address: z.string().max(500).optional().nullable(),
  shipping_address: z.string().max(500).optional().nullable(),
  contact_name: z.string().max(150).optional().nullable(),
  contact_email: z.string().email().optional().nullable(),
  contact_phone: z.string().max(40).optional().nullable(),
  credit_limit_thb: z.coerce.number().min(0).optional(),
  payment_terms: z.string().max(40).optional(),
});

const BlacklistForm = z.object({
  id: z.coerce.number().int().positive(),
  blacklist: z.union([z.literal('true'), z.literal('false'), z.literal('on'), z.literal('off')]),
});

export async function createCustomerAction(formData: FormData): Promise<{ ok: boolean; id?: number; error?: string }> {
  const actor = await loadActor();
  if (!actor) return { ok: false, error: 'unauthorized' };
  if (!matchPerm(actor.permissions, 'customer:manage::allow')) {
    return { ok: false, error: 'forbidden' };
  }

  const parsed = CreateForm.safeParse({
    code: String(formData.get('code') ?? '').trim().toUpperCase(),
    name: String(formData.get('name') ?? '').trim(),
    name_th: formData.get('name_th') ? String(formData.get('name_th')).trim() : null,
    tax_id: formData.get('tax_id') ? String(formData.get('tax_id')).trim() : null,
    billing_address: formData.get('billing_address') ? String(formData.get('billing_address')).trim() : null,
    shipping_address: formData.get('shipping_address') ? String(formData.get('shipping_address')).trim() : null,
    contact_name: formData.get('contact_name') ? String(formData.get('contact_name')).trim() : null,
    contact_email: formData.get('contact_email') ? String(formData.get('contact_email')).trim() : null,
    contact_phone: formData.get('contact_phone') ? String(formData.get('contact_phone')).trim() : null,
    credit_limit_thb: formData.get('credit_limit_thb') ?? 0,
    payment_terms: formData.get('payment_terms') ?? 'Net 30',
  });
  if (!parsed.success) return { ok: false, error: parsed.error.message };
  try {
    const created = await serverCreate({
      code: parsed.data.code,
      name: parsed.data.name,
      credit_limit_thb: parsed.data.credit_limit_thb,
      payment_terms: parsed.data.payment_terms,
      name_th: parsed.data.name_th ?? null,
      tax_id: parsed.data.tax_id ?? null,
      billing_address: parsed.data.billing_address ?? null,
      shipping_address: parsed.data.shipping_address ?? null,
      contact_name: parsed.data.contact_name ?? null,
      contact_email: parsed.data.contact_email ?? null,
      contact_phone: parsed.data.contact_phone ?? null,
      blacklist: false,
      is_active: true,
    });
    revalidatePath('/customers');
    return { ok: true, id: created.id };
  } catch (e: any) {
    return { ok: false, error: e?.message ?? 'unknown' };
  }
}

export async function updateCustomerAction(formData: FormData): Promise<{ ok: boolean; error?: string }> {
  const actor = await loadActor();
  if (!actor) return { ok: false, error: 'unauthorized' };
  if (!matchPerm(actor.permissions, 'customer:manage::allow')) {
    return { ok: false, error: 'forbidden' };
  }
  const parsed = UpdateForm.safeParse({
    id: formData.get('id'),
    name: formData.get('name') ? String(formData.get('name')).trim() : undefined,
    name_th: formData.get('name_th') ? String(formData.get('name_th')).trim() : null,
    tax_id: formData.get('tax_id') ? String(formData.get('tax_id')).trim() : null,
    billing_address: formData.get('billing_address') ? String(formData.get('billing_address')).trim() : null,
    shipping_address: formData.get('shipping_address') ? String(formData.get('shipping_address')).trim() : null,
    contact_name: formData.get('contact_name') ? String(formData.get('contact_name')).trim() : null,
    contact_email: formData.get('contact_email') ? String(formData.get('contact_email')).trim() : null,
    contact_phone: formData.get('contact_phone') ? String(formData.get('contact_phone')).trim() : null,
    credit_limit_thb: formData.get('credit_limit_thb') ? formData.get('credit_limit_thb') : undefined,
    payment_terms: formData.get('payment_terms') ? String(formData.get('payment_terms')) : undefined,
  });
  if (!parsed.success) return { ok: false, error: parsed.error.message };
  const { id, ...patch } = parsed.data;
  try {
    const updated = await serverUpdate(id, patch);
    if (!updated) return { ok: false, error: 'not found' };
    revalidatePath(`/customers/${id}`);
    revalidatePath('/customers');
    return { ok: true };
  } catch (e: any) {
    return { ok: false, error: e?.message ?? 'unknown' };
  }
}

export async function blacklistCustomerAction(formData: FormData): Promise<{ ok: boolean; error?: string }> {
  const actor = await loadActor();
  if (!actor) return { ok: false, error: 'unauthorized' };
  if (!matchPerm(actor.permissions, 'customer:manage::allow')) {
    return { ok: false, error: 'forbidden' };
  }
  const parsed = BlacklistForm.safeParse({
    id: formData.get('id'),
    blacklist: formData.get('blacklist'),
  });
  if (!parsed.success) return { ok: false, error: parsed.error.message };
  const value = parsed.data.blacklist === 'true' || parsed.data.blacklist === 'on';
  try {
    await serverBlacklist(parsed.data.id, value);
    revalidatePath(`/customers/${parsed.data.id}`);
    revalidatePath('/customers');
    return { ok: true };
  } catch (e: any) {
    return { ok: false, error: e?.message ?? 'unknown' };
  }
}
