import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";

import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { ecommerceDetail } from "@erpx/server-ecommerce/public/ecommerce";
import { EcommerceCheckoutControls } from "@erpx/feature-records/components/loaders/modules--ecommerce-checkout-controls";
import { PageContent, Card } from "@erpx/ui/components/foundation";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    actor = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(actor));
  const data = await ecommerceDetail(actor, id).catch((error) =>
    pageError(error, holdingSlug),
  );
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={canonicalUiHref(`${base}/ecommerce`)}>E-commerce</Link>
      <h1>{String(data.order.order.number)}</h1>
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Checkout</h2>
        <p>Status: {String(data.checkout.status)}</p>
        <p>Sales owner status: {String(data.order.order.state)}</p>
        <p>Checkout version: {String(data.checkout.version)}</p>
      </Card>
      <EcommerceCheckoutControls base={base} data={data} />
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Fulfillment links</h2>
        {data.fulfillmentLinks.map((x) => (
          <p key={String(x.id)}>
            {String(x.fulfillment_id)} · {String(x.status)}
          </p>
        ))}
      </Card>
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Return links</h2>
        {data.returnLinks.map((x) => (
          <p key={String(x.id)}>
            {String(x.commerce_return_id)} · {String(x.current_return_status)}
          </p>
        ))}
      </Card>
      <UiActionLink appearance="standard" variant="primary"

        href={canonicalUiHref(`${base}/commerce/orders/${data.checkout.sales_order_id}`)}
      >
        Open Sales order
      </UiActionLink>
    </PageContent>
  );
}
