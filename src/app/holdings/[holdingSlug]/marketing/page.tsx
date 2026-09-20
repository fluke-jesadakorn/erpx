import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/page-error";
import { marketingWorkspace } from "@/server/marketing";
import { MarketingConsole } from "@/components/modules/marketing-console";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    data = await marketingWorkspace(a).catch((e) => pageError(e, holdingSlug));
  return (
    <div className="content-stack">
      <Link href={`${base}/crm`}>CRM</Link>
      <h1>Marketing</h1>
      <p>
        Review forms, content, domains and provider readiness before capturing
        consented, attributed leads for CRM.
      </p>
      <MarketingConsole base={base} data={data} />
      <section className="card">
        <h2>Lead candidates</h2>
        {data.candidates.map((x) => (
          <Link
            className="row"
            key={String(x.id)}
            href={`${base}/marketing/${x.id}`}
          >
            {String(x.contact_name)} · {String(x.form_name)} ·{" "}
            {String(x.status)}
          </Link>
        ))}
      </section>
    </div>
  );
}
