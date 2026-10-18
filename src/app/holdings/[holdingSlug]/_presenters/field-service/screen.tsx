import {serviceCustomerPreparation} from '@/server/modules/finance/public/service-customer-preparation';
import {ServiceCustomerPreparation} from '@/components/field_service/customer-preparation';
import {hasPermission} from "@/lib/identity_access/permissions";
import {servicePartsStockWorkspace} from "@/server/modules/commerce/public/service-parts-posting-setup";
import {servicePartsPrerequisites} from "@/server/modules/finance/public/service-parts-prerequisites";
import {ServicePartsEntryLink} from "@/components/field_service/parts-entry-link";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { RecordLink as Link } from "@/components/records/record-link";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { fieldServiceWorkspace } from "@/server/modules/field_service/public/field-service";
import { FieldServiceConsole } from "@/components/field_service/field-service-console";
import { PageContent, Card } from "@/components/ui/foundation";
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
  const stockRole=!a.beta&&!a.departmentId&&hasPermission(a,"inventory.read")&&hasPermission(a,"inventory.prepare"),financeRole=!a.beta&&a.scope.kind==="holding"&&!a.branchId&&!a.departmentId&&hasPermission(a,"finance.read")&&["finance.write","finance.approve"].some(permission=>hasPermission(a,permission));
  const partsSetupAvailable=stockRole?(await servicePartsStockWorkspace(a).catch(e=>pageError(e,holdingSlug))).available:financeRole?(await servicePartsPrerequisites(a).catch(e=>pageError(e,holdingSlug))).available:false;
  const customers=await serviceCustomerPreparation(a).catch(e=>pageError(e,holdingSlug));
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={canonicalUiHref(`${base}/work`)}>Work</Link>
      <h1>Field service</h1>
      <p>
        Route customer cases through agreements, dispatch, parts, visit evidence
        and independent billing release.
      </p>
      {partsSetupAvailable&&<ServicePartsEntryLink base={base}/>}
      {(customers.available||data.customers.length===0)&&<ServiceCustomerPreparation base={base} workspaceKey={data.workspaceKey} initial={customers}/>}
      <FieldServiceConsole base={base} data={data} />
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Service orders</h2>
        {data.orders.map((x) => (
          <Link
            className="ui-record-item"
            key={String(x.id)}
            href={canonicalUiHref(`${base}/field-service/${x.id}`)}
          >
            {String(x.ticket_title)} · {String(x.customer_name)} ·{" "}
            {String(x.status)}
          </Link>
        ))}
        {!data.orders.length && <p>No service orders in this scope.</p>}
      </Card>
    </PageContent>
  );
}
