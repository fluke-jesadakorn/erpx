import {ServiceParts} from '@/components/field_service/parts-setup';
import {servicePartsPostingSetup,servicePartsStockWorkspace} from '@/server/modules/commerce/public/service-parts-posting-setup';
import {servicePartsPrerequisites} from '@/server/modules/finance/public/service-parts-prerequisites';
import {requireWorkContext,publicScope} from '@/server/modules/identity_access/public/authorization';
import {workspaceBase} from '@/lib/work_projection/workspace';
import {pageError} from '@/server/modules/work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),[mapping,prerequisites,workspace]=await Promise.all([servicePartsPostingSetup(actor),servicePartsPrerequisites(actor),servicePartsStockWorkspace(actor)]).catch(error=>pageError(error,holdingSlug));
 return <ServiceParts base={workspaceBase(publicScope(actor))} initial={{mapping,prerequisites,workspace}}/>;
}
