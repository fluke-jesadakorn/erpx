import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/page-error";
import { fieldServiceWorkspace } from "@/server/field-service";
import { FieldServiceConsole } from "@/components/modules/field-service-console";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    data = await fieldServiceWorkspace(a).catch((e) =>
      pageError(e, holdingSlug),
    );
  return (
    <div className="content-stack">
      <Link href={`${base}/work`}>Work</Link>
      <h1>Field service</h1>
      <p>
        Route customer cases through agreements, dispatch, parts, visit evidence
        and independent billing release.
      </p>
      <FieldServiceConsole base={base} data={data} />
      <section className="card">
        <h2>Service orders</h2>
        {data.orders.map((x) => (
          <Link
            className="row"
            key={String(x.id)}
            href={`${base}/field-service/${x.id}`}
          >
            {String(x.ticket_title)} · {String(x.customer_name)} ·{" "}
            {String(x.status)}
          </Link>
        ))}
        {!data.orders.length && <p>No service orders in this scope.</p>}
      </section>
    </div>
  );
}
