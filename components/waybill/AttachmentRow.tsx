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
    <div className="flex flex-col gap-1.5 rounded-lg border border-rule bg-paper-2/50 p-3">
      <div className="flex items-start gap-3">
        <span aria-hidden className="text-2xl">{kindMeta.emoji}</span>
        <div className="flex-1 space-y-0.5">
          <div className="flex items-baseline justify-between gap-2">
            <a
              href={downloadHref}
              className="break-all font-mono text-xs text-info underline-offset-2 hover:underline"
            >
              {attachment.filename}
            </a>
            <span className="shrink-0 font-mono text-xs text-mute">
              {fmtSize(attachment.byte_size)}
            </span>
          </div>
          <div className="flex flex-wrap items-center gap-2 text-xs font-mono text-ink-2">
            <span className="rounded bg-paper-2 px-1.5 py-0.5 text-ink-2">
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
            <p className="text-xs italic text-ink-2">“{attachment.caption}”</p>
          )}
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-2 pl-9">
        <a
          href={downloadHref}
          className="rounded bg-info px-2 py-1 text-xs font-mono text-info-soft hover:bg-info"
        >
          ⤓ Download
        </a>
        {canRemove && (
          <form action={removeWaybillAttachmentAction}>
            <input type="hidden" name="waybillId" value={waybillId} />
            <input type="hidden" name="attachmentId" value={attachment.id} />
            <button
              type="submit"
              className="rounded bg-critical px-2 py-1 text-xs font-mono text-critical-soft hover:bg-critical"
            >
              ✕ Remove
            </button>
          </form>
        )}
      </div>
    </div>
  );
}
