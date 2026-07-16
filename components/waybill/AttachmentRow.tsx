import React from 'react';
import type { WaybillAttachmentRow } from '@/waybill/attachments';
import { WAYBILL_KINDS, type WaybillAttachmentKind } from '@/waybill/kinds';
import { loadActor } from '@/server/guard';
import { canActorRemoveAttachment } from '@/waybill/permissions';
import { removeWaybillAttachmentAction } from '@/app/actions/waybill';

export interface AttachmentRowProps {
  waybillId: string;
  attachment: WaybillAttachmentRow;
  showRemoveButton?: boolean;
}

function fmtSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(2)} MB`;
}

function fmtTime(t: Date | string): string {
  const d = t instanceof Date ? t : new Date(t);
  if (!Number.isFinite(d.getTime())) return '—';
  return d.toLocaleString();
}

export async function AttachmentRow({ waybillId, attachment }: AttachmentRowProps) {
  const actor = await loadActor();
  const canRemove = actor
    ? canActorRemoveAttachment({ id: actor.id, roleName: actor.role_name ?? '' })
    : false;

  const kindMeta = WAYBILL_KINDS[attachment.kind as WaybillAttachmentKind] ?? WAYBILL_KINDS.other;
  let downloadHref = `/api/waybill/${waybillId}/attachments/file?key=${encodeURIComponent(attachment.storage_key)}`;
  try {
    downloadHref = `/api/slips/file?key=${encodeURIComponent(attachment.storage_key)}`;
  } catch {
    downloadHref = '#';
  }

  return (
    <div className="flex flex-col gap-1.5 rounded-lg border border-slate-800 bg-slate-950/40 p-3">
      <div className="flex items-start gap-3">
        <span aria-hidden className="text-2xl">{kindMeta.emoji}</span>
        <div className="flex-1 space-y-0.5">
          <div className="flex items-baseline justify-between gap-2">
            <a
              href={downloadHref}
              className="break-all font-mono text-xs text-cyan-300 underline-offset-2 hover:underline"
            >
              {attachment.filename}
            </a>
            <span className="shrink-0 font-mono text-xs text-slate-500">
              {fmtSize(attachment.byte_size)}
            </span>
          </div>
          <div className="flex flex-wrap items-center gap-2 text-xs font-mono text-slate-400">
            <span className="rounded bg-slate-800 px-1.5 py-0.5 text-slate-300">
              {attachment.kind}
            </span>
            <span>·</span>
            <span>stage: {attachment.stage_key}</span>
            <span>·</span>
            <span>by {attachment.uploaded_role} #{attachment.uploaded_by}</span>
            <span>·</span>
            <span>{fmtTime(attachment.occurred_at)}</span>
          </div>
          {attachment.caption && (
            <p className="text-xs italic text-slate-400">“{attachment.caption}”</p>
          )}
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-2 pl-9">
        <a
          href={downloadHref}
          className="rounded bg-cyan-500/15 px-2 py-1 text-xs font-mono text-cyan-200 hover:bg-cyan-500/30"
        >
          ⤓ Download
        </a>
        {canRemove && (
          <form action={removeWaybillAttachmentAction}>
            <input type="hidden" name="waybillId" value={waybillId} />
            <input type="hidden" name="attachmentId" value={attachment.id} />
            <button
              type="submit"
              className="rounded bg-rose-500/10 px-2 py-1 text-xs font-mono text-rose-200 hover:bg-rose-500/30"
            >
              ✕ Remove
            </button>
          </form>
        )}
      </div>
    </div>
  );
}
