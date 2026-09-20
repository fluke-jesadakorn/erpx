import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/page-error";
import { productionWorkspace } from "@/server/production";
import { ProductionConsole } from "@/components/modules/production-console";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    data = await productionWorkspace(a, "manufacturing").catch((e) =>
      pageError(e, holdingSlug),
    );
  return (
    <div className="content-stack">
      <Link href={`${base}/planning`}>Planning</Link>
      <h1>Manufacturing</h1>
      <p>
        Control BOM and formula production from material reservation through
        output, QC, cost and genealogy.
      </p>
      <ProductionConsole base={base} module="manufacturing" data={data} />
      <section className="card">
        <h2>Orders</h2>
        {data.orders.map((x) => (
          <Link
            className="row"
            key={String(x.id)}
            href={`${base}/manufacturing/${x.id}`}
          >
            {String(x.specification_name)} · {String(x.status)}
          </Link>
        ))}
      </section>
    </div>
  );
}
