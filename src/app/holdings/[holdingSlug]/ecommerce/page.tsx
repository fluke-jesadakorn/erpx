import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/page-error";
import { ecommerceWorkspace } from "@/server/ecommerce";
import { EcommerceConsole } from "@/components/modules/ecommerce-console";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params,
    actor = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(actor));
  const data = await ecommerceWorkspace(actor).catch((error) =>
    pageError(error, holdingSlug),
  );
  return (
    <div className="content-stack">
      <Link href={`${base}/commerce`}>Sales and stock</Link>
      <h1>E-commerce</h1>
      <p>
        Approve sellable channels and listings, lock the customer cart into a
        Sales order, reserve stock, and trace fulfillment and returns to their
        owner records.
      </p>
      <EcommerceConsole base={base} data={data} />
      <section className="card">
        <h2>Checkout handoffs</h2>
        {data.checkouts.map((x) => (
          <Link
            className="row"
            key={String(x.id)}
            href={`${base}/ecommerce/${x.id}`}
          >
            {String(x.number)} · {String(x.status)} · Sales{" "}
            {String(x.order_state)}
          </Link>
        ))}
      </section>
    </div>
  );
}
