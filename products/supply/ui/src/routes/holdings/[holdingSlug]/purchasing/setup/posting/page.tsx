import {CommercePostingSetup} from '@erpx/commerce-ui/components/commerce-posting-setup';
import {requireWorkContext,publicScope} from '@erpx/identity-access-server/public/authorization';
import {commercePostingSetup} from '@erpx/commerce-server/public/posting-setup';
import {supplyPostingPrerequisites} from '@erpx/finance-server/public/supply-posting-prerequisites';
import {workspaceBase} from '@erpx/work-projection-ui/lib/workspace';
import {pageError} from '@erpx/work-projection-server/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),[initial,initialPrerequisites]=await Promise.all([commercePostingSetup(actor),supplyPostingPrerequisites(actor)]).catch(error=>pageError(error,holdingSlug));
 return <CommercePostingSetup base={workspaceBase(publicScope(actor))} initial={initial} initialPrerequisites={initialPrerequisites}/>;
}
