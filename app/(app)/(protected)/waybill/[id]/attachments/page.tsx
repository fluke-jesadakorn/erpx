import 'server-only';
import { notFound, redirect } from 'next/navigation';
import { loadWaybillRailContext, loadAttachmentsForWaybill } from '@/waybill/queries';
import { loadActor } from '@/server/guard';
import { AttachmentRow } from '@/components/waybill/AttachmentRow';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { crumbsForPath } from '@/components/breadcrumbs/routes';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';
import { OverviewShell } from '../_components/Overview';
import { Panel, Badge, Icon, Empty } from '@/components/ui';

export const dynamic = 'force-dynamic';

interface PageProps {
  params: Promise<{ id: string }>;
}

export default async function WaybillAttachmentsPage({ params }: PageProps) {
  const { id } = await params;
  const actor = await loadActor();
  if (!actor) redirect('/login');

  const ctx = await loadWaybillRailContext(id);
  if (!ctx) notFound();
  const wb = ctx.waybill;
  const locale = await getSecondaryLocale();

  const attachments = await loadAttachmentsForWaybill(wb.id);
  const totalSize = attachments.reduce((s, a) => s + Number(a.byte_size ?? 0), 0);
  const byStage = attachments.reduce<Record<string, typeof attachments>>((acc, a) => {
    (acc[a.stage_key] ||= []).push(a);
    return acc;
  }, {});

  return (
    <>
      <BreadcrumbSetter
        crumbs={crumbsForPath(`/waybill/${wb.id}/attachments`, locale, { waybillId: wb.id, subtab: 'attachments' })}
      />
      <PageLayout title={`Attachments · ${wb.id}`} subtitle={`${attachments.length} files`}>
        <OverviewShell waybillId={wb.id} active="attachments">
          {attachments.length === 0 ? (
            <Empty
              icon="paperclip"
              title={<T id="waybill.timeline.no_documents_at_this_pip" />}
            />
          ) : (
            <div className="space-y-4">
              <div className="flex items-center gap-2 text-xs font-mono uppercase tracking-widest text-mute">
                <Icon name="paperclip" size={12} aria-hidden />
                <span>Total</span>
                <Badge tone="neutral" size="sm">{attachments.length}</Badge>
                <span>·</span>
                <span>{formatBytes(totalSize)}</span>
              </div>
              {Object.entries(byStage).map(([stage, list]) => (
                <Panel key={stage} padding="md" className="space-y-4">
                  <h3 className="text-xs font-mono uppercase tracking-widest text-accent">
                    Stage · <span className="text-ink">{stage}</span>
                    <Badge tone="neutral" size="sm" className="ml-2">{list.length}</Badge>
                  </h3>
                  <div className="divide-y divide-rule rounded-lg border border-rule bg-paper-2">
                    {list.map((a) => (
                      <AttachmentRow key={a.id} waybillId={wb.id} attachment={a} />
                    ))}
                  </div>
                </Panel>
              ))}
            </div>
          )}
        </OverviewShell>
      </PageLayout>
    </>
  );
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(2)} MB`;
}