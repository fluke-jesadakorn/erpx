import React from 'react';
import { loadAttachmentsForWaybill } from '@folio-lib/waybill/queries';
import type { SecondaryLocale } from '@folio-lib/server/locale';
import { ZoneSection } from '../ZoneSection';
import { AttachmentRow } from '../AttachmentRow';
import { fmtSize } from '../ui';
import { bi } from '@/components/i18n/bi';

interface Props {
  waybillId: string;
  pipKey: string;
  locale?: SecondaryLocale;
}

export async function PipDocuments({ waybillId, pipKey, locale }: Props) {
  const localeSafe: SecondaryLocale = locale ?? 'th';
  const all = await loadAttachmentsForWaybill(waybillId);
  const pipAttachments = all.filter((a) => a.stage_key === pipKey);
  if (pipAttachments.length === 0) return null;
  const totalSize = pipAttachments.reduce((s, a) => s + Number(a.byte_size ?? 0), 0);

  return (
    <ZoneSection
      icon={<span aria-hidden>📎</span>}
      label={bi('Documents', 'เอกสาร', undefined, localeSafe)}
      count={fmtSize(totalSize)}
      meta={
        pipAttachments.length > 0
          ? bi(
              `${pipAttachments.length} file${pipAttachments.length === 1 ? '' : 's'}`,
              `รวม ${pipAttachments.length} ไฟล์`,
              undefined,
              localeSafe,
            )
          : undefined
      }
      tone="indigo"
    >
      <div className="divide-y divide-slate-800/40">
        {pipAttachments.map((a) => (
          <AttachmentRow key={a.id} waybillId={waybillId} attachment={a} />
        ))}
      </div>
    </ZoneSection>
  );
}
