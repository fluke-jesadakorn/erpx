import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
import { requireWorkContext, publicScope } from "@erpx/identity-access-server/public/authorization";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { pageError } from "@erpx/work-projection-server/public/page-error";
import { marketingDetail } from "@erpx/marketing-server/public/marketing";
import {
  nativeMarketingLeadDetail,
  nativeMarketingLeadFailure,
} from "@erpx/marketing-ui/public/native-marketing-lead";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { headers } from "next/headers";
import { MarketingConsole } from "@erpx/records-ui/components/loaders/modules--marketing-console";
import { PageContent, Card } from "@erpx/platform-ui/components/foundation";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    native = await nativeMarketingLeadDetail(await headers(), a, id).catch((e) => {
      const failure = nativeMarketingLeadFailure(e);
      if (failure) throw new ApiError(failure.status, failure.code, failure.message);
      return pageError(e, holdingSlug);
    }),
    data = (native ?? (await marketingDetail(a, id).catch((e) => pageError(e, holdingSlug)))) as Awaited<
      ReturnType<typeof marketingDetail>
    >,
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
