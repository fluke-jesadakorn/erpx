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
    data = await productionWorkspace(a, "planning").catch((e) =>
      pageError(e, holdingSlug),
    );
  return (
    <div className="content-stack">
      <Link href={`${base}/manufacturing`}>Manufacturing</Link>
      <h1>Planning</h1>
      <p>
        Calculate traceable shortages, capacity and feasible dates, then
        independently release recommendations.
      </p>
      <ProductionConsole base={base} module="planning" data={data} />
    </div>
  );
}
