import {headers} from 'next/headers';
import {nativePartsPrerequisites} from '@erpx/feature-field_service/public/native-parts-setup';
import {nativeServiceWorkspace,NativeServiceWorkspaceError} from '@erpx/feature-field_service/public/native-workspace';
import {ApiError} from '@erpx/server-runtime/public/http';
import {serviceCustomerPreparation} from '@erpx/server-finance/public/service-customer-preparation';
import {ServiceCustomerPreparation} from '@erpx/feature-field_service/components/customer-preparation';
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {servicePartsStockWorkspace} from "@erpx/server-commerce/public/service-parts-posting-setup";
import {servicePartsPrerequisites} from "@erpx/server-finance/public/service-parts-prerequisites";
import {ServicePartsEntryLink} from "@erpx/feature-field_service/components/parts-entry-link";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { fieldServiceWorkspace } from "@erpx/server-field_service/public/field-service";
import { FieldServiceConsole } from "@erpx/feature-field_service/components/field-service-console";
import { PageContent, Card } from "@erpx/ui/components/foundation";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a));
  const context=a.authority && a.iam ? {
    userId:a.userId,holdingId:a.holdingId,companyId:a.companyId,branchId:a.branchId,departmentId:a.departmentId,
    branchSlug:a.scope.kind==='holding'?null:a.scope.branchSlug??null,departmentSlug:a.scope.kind==='department'?a.scope.departmentSlug:null,
    assignmentId:a.assignmentId,assignmentVersion:a.authority.assignmentVersion,revision:a.authority.revision,
    role:a.role,assignmentScopeKind:a.assignmentScope,modules:[...a.modules],permissions:[...a.iam.permissions],
  }:undefined;
  const data=await (async()=>await nativeServiceWorkspace(holdingSlug,context)??await fieldServiceWorkspace(a))().catch(e=>{
    if(e instanceof NativeServiceWorkspaceError && [401,403,404,409].includes(e.status??0))return pageError(new ApiError(e.status!,e.code,'The Service workspace is unavailable.'),holdingSlug);
    if(e instanceof NativeServiceWorkspaceError)throw e;
    return pageError(e,holdingSlug);
  });
  const stockRole=!a.beta&&!a.departmentId&&hasPermission(a,"inventory.read")&&hasPermission(a,"inventory.prepare"),financeRole=!a.beta&&a.scope.kind==="holding"&&!a.branchId&&!a.departmentId&&hasPermission(a,"finance.read")&&["finance.write","finance.approve"].some(permission=>hasPermission(a,permission));
  const partsSetupAvailable=stockRole?(await servicePartsStockWorkspace(a).catch(e=>pageError(e,holdingSlug))).available:financeRole?(await (async()=>await nativePartsPrerequisites(await headers(),holdingSlug,context,'read')??await servicePartsPrerequisites(a))().catch(e=>pageError(e,holdingSlug))).available:false;
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
      <FieldServiceConsole base={base} data={data} nativeReserve={process.env.ERPX_NATIVE_FIELD_SERVICE_REQUEST_CONFIG === undefined && process.env.ERPX_NATIVE_FIELD_SERVICE_ORDER_CREATE_CONFIG === undefined && process.env.ERPX_NATIVE_FIELD_SERVICE_AGREEMENT_REVIEW_CONFIG === undefined && process.env.ERPX_NATIVE_FIELD_SERVICE_AGREEMENT_CREATE_CONFIG === undefined ? undefined : {agreementCreate:process.env.ERPX_NATIVE_FIELD_SERVICE_AGREEMENT_CREATE_CONFIG !== undefined,agreementReview:process.env.ERPX_NATIVE_FIELD_SERVICE_AGREEMENT_REVIEW_CONFIG !== undefined,request:process.env.ERPX_NATIVE_FIELD_SERVICE_REQUEST_CONFIG !== undefined,create:process.env.ERPX_NATIVE_FIELD_SERVICE_ORDER_CREATE_CONFIG !== undefined,reserve:false,context}} />
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
