import {CommercePostingSetup} from '@erpx/feature-commerce/components/commerce-posting-setup';
import {requireWorkContext,publicScope} from '@erpx/server-identity_access/public/authorization';
import {commercePostingSetup} from '@erpx/server-commerce/public/posting-setup';
import {supplyPostingPrerequisites} from '@erpx/server-finance/public/supply-posting-prerequisites';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {pageError} from '@erpx/server-work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),[initial,initialPrerequisites]=await Promise.all([commercePostingSetup(actor),supplyPostingPrerequisites(actor)]).catch(error=>pageError(error,holdingSlug));
 return <CommercePostingSetup base={workspaceBase(publicScope(actor))} initial={initial} initialPrerequisites={initialPrerequisites}/>;
}
