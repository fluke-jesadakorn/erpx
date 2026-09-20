import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/page-error";
import { marketingDetail } from "@/server/marketing";
import { MarketingConsole } from "@/components/modules/marketing-console";
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
    <div className="content-stack">
      <Link href={`${base}/marketing`}>Marketing</Link>
      <h1>{String(x.contact_name)}</h1>
      <p>
        {String(x.form_name)} · {String(x.status)}
      </p>
      <MarketingConsole base={base} detail={data} />
      <section className="card">
        <h2>Engagement history</h2>
        {data.history.map((h) => (
          <p key={String(h.id)}>
            Version {String(h.version)} · {String(h.event_type)} ·{" "}
            {String(h.channel)} · {String(h.evidence)}
          </p>
        ))}
      </section>
      {x.crm_lead_id && (
        <Link href={`${base}/crm/${x.crm_lead_id}`}>Open CRM lead</Link>
      )}
    </div>
  );
}
