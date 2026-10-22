import {ServiceParts} from '@erpx/feature-field_service/components/parts-setup';
import {servicePartsPostingSetup,servicePartsStockWorkspace} from '@erpx/server-commerce/public/service-parts-posting-setup';
import {servicePartsPrerequisites} from '@erpx/server-finance/public/service-parts-prerequisites';
import {requireWorkContext,publicScope} from '@erpx/server-identity_access/public/authorization';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {pageError} from '@erpx/server-work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),[mapping,prerequisites,workspace]=await Promise.all([servicePartsPostingSetup(actor),servicePartsPrerequisites(actor),servicePartsStockWorkspace(actor)]).catch(error=>pageError(error,holdingSlug));
 return <ServiceParts base={workspaceBase(publicScope(actor))} initial={{mapping,prerequisites,workspace}}/>;
}
