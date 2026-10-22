import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { marketingDetail } from "@erpx/server-marketing/public/marketing";
import { MarketingConsole } from "@erpx/feature-records/components/loaders/modules--marketing-console";
import { PageContent, Card } from "@erpx/ui/components/foundation";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    data = await marketingDetail(a, id).catch((e) => pageError(e, holdingSlug)),
    x = data.candidate;
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={canonicalUiHref(`${base}/marketing`)}>Marketing</Link>
      <h1>{String(x.contact_name)}</h1>
      <p>
        {String(x.form_name)} · {String(x.status)}
      </p>
      <MarketingConsole base={base} detail={data} />
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Engagement history</h2>
        {data.history.map((h) => (
          <p key={String(h.id)}>
            Version {String(h.version)} · {String(h.event_type)} ·{" "}
            {String(h.channel)} · {String(h.evidence)}
          </p>
        ))}
      </Card>
      {x.crm_lead_id && (
        <Link href={canonicalUiHref(`${base}/crm/${x.crm_lead_id}`)}>Open CRM lead</Link>
      )}
    </PageContent>
  );
}
