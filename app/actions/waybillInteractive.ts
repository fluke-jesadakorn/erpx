'use server';

import { invoke } from '@/ai/router';
import { hasPermission } from '@folio-lib/perm/server';
import { PERM } from '@folio-lib/perm/taxonomy';
import { requireActor } from '@/server/guard';
import { getSecondaryLocale } from '@/server/locale';
import { makeKey, presignedPutUrl } from '@/slips/storage';
import { loadWaybill } from '@/waybill/queries';
import { canActorAttachAt, isTerminalStage } from '@/waybill/permissions';
import { allowedKindsFor, type WaybillAttachmentKind } from '@/waybill/kinds';

const VALID_KINDS: ReadonlySet<string> = new Set([
  'slip', 'pr_doc', 'po_doc', 'payment_receipt', 'signoff_memo',
  'invoice', 'wht_cert', 'photo', 'memo', 'other',
]);

function stripThink(s: string): string {
  return (s ?? '').replace(/<\/?think[^>]*>/g, '').trim();
}

function langOf(formData: FormData, fallback: 'th' | 'de'): 'en' | 'th' | 'de' {
  const raw = String(formData.get('lang') ?? '');
  if (raw === 'en' || raw === 'th' || raw === 'de') return raw;
  return fallback === 'de' ? 'de' : 'th';
}

export async function explainWaybillEventAction(
  _prev: unknown,
  formData: FormData,
): Promise<{ ok: boolean; text?: string; error?: string; modelName?: string; latencyMs?: number }> {
  let actor;
  try {
    actor = await requireActor();
  } catch {
    return { ok: false, error: 'unauthorized' };
  }
  const allowed =
    hasPermission(actor, PERM.finance.expense.view_all) ||
    hasPermission(actor, PERM.finance.expense.view_own) ||
    hasPermission(actor, PERM.admin.system.bypass);
  if (!allowed) return { ok: false, error: 'forbidden' };

  const waybillId = String(formData.get('waybillId') ?? '').trim();
  const eventKind = String(formData.get('eventKind') ?? '').trim();
  const fromStage = String(formData.get('fromStage') ?? '').trim();
  const toStage = String(formData.get('toStage') ?? '').trim();
  const rawName = String(formData.get('actorName') ?? '').trim();
  const actorName = rawName.length > 0 ? rawName : 'unknown actor';
  if (!waybillId || !eventKind || !fromStage || !toStage) {
    return { ok: false, error: 'waybillId, eventKind, fromStage, toStage are required' };
  }

  const fallbackLang = await getSecondaryLocale();
  const lang = langOf(formData, fallbackLang);

  const languageHint =
    lang === 'th' ? 'เขียนคำตอบเป็นภาษาไทย' :
    lang === 'de' ? 'Schreiben Sie die Antwort auf Deutsch.' :
    'Write the answer in English.';

  const systemPrompt =
    `You are explaining a single waybill event to a Thai office worker. ` +
    `Event kind=${eventKind}, from stage=${fromStage}, to stage=${toStage}, actor=${actorName}. ` +
    `Output 2 short sentences in the same language as the question: ` +
    `(1) what happened, (2) what the next state will trigger or require. ` +
    `Prose, no bullets. ${languageHint}`;

  const userText =
    lang === 'th'
      ? `ช่วยอธิบายเหตุการณ์นี้ให้พนักงานไทยเข้าใจง่าย`
      : lang === 'de'
        ? `Erklären Sie dieses Waybill-Ereignis in einfacher Sprache.`
        : `Explain this waybill event in plain language.`;

  const result = await invoke(
    'events:explain',
    'chat',
    { text: userText, systemPrompt, temperature: 0.3 },
    { actorId: actor.id },
  );

  if (!result.ok || !result.text) {
    return {
      ok: false,
      error: result.error || 'AI unavailable',
      modelName: result.modelName,
      latencyMs: result.latencyMs ?? 0,
    };
  }

  return {
    ok: true,
    text: stripThink(result.text),
    modelName: result.modelName ?? 'unknown',
    latencyMs: result.latencyMs ?? 0,
  };
}

export async function presignWaybillAttachmentAction(
  _prev: unknown,
  formData: FormData,
): Promise<{ ok: boolean; key?: string; put_url?: string; expires?: number; kind?: string; content_type?: string; filename?: string; error?: string }> {
  let actor;
  try {
    actor = await requireActor();
  } catch {
    return { ok: false, error: 'unauthorized' };
  }
  const allowed =
    hasPermission(actor, PERM.finance.expense.view_own) ||
    hasPermission(actor, PERM.finance.expense.view_all) ||
    hasPermission(actor, PERM.admin.system.bypass) ||
    hasPermission(actor, 'finance:waybill:attach::allow');
  if (!allowed) return { ok: false, error: 'forbidden' };

  const waybillId = String(formData.get('waybillId') ?? '').trim();
  const filename = String(formData.get('filename') ?? '').trim();
  const contentType = String(formData.get('contentType') ?? 'application/octet-stream').trim();
  const kindRaw = String(formData.get('kind') ?? '').trim();

  if (!waybillId) return { ok: false, error: 'waybillId required' };
  if (!filename || !contentType || !kindRaw) {
    return { ok: false, error: 'filename, contentType, kind required' };
  }
  if (!VALID_KINDS.has(kindRaw)) {
    return { ok: false, error: `invalid kind '${kindRaw}'` };
  }
  const kind = kindRaw as WaybillAttachmentKind;

  const wb = await loadWaybill(waybillId);
  if (!wb) return { ok: false, error: 'waybill not found' };

  if (isTerminalStage(wb.current_stage)) {
    return { ok: false, error: `cannot attach to ${wb.current_stage}` };
  }
  if (!canActorAttachAt(actor.role_name, wb.current_stage)) {
    return { ok: false, error: `role '${actor.role_name}' cannot attach at stage '${wb.current_stage}'` };
  }
  if (!allowedKindsFor(wb.current_stage).includes(kind)) {
    return { ok: false, error: `kind '${kind}' not allowed at stage '${wb.current_stage}'` };
  }

  const safeBase = filename.replace(/[^a-zA-Z0-9._-]/g, '_').slice(0, 80);
  const random = makeKey(safeBase);
  const key = `waybill-attachments/${waybillId}/${wb.current_stage}/${random}`;
  const expires = 900;

  try {
    const putUrl = await presignedPutUrl(key, expires);
    return {
      ok: true,
      key,
      put_url: putUrl,
      expires,
      kind,
      content_type: contentType,
      filename: safeBase,
    };
  } catch (e: any) {
    return { ok: false, error: e?.message || 'presign failed' };
  }
}
