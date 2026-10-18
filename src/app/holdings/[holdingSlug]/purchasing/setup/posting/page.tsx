import {CommercePostingSetup} from '@/components/commerce/commerce-posting-setup';
import {requireWorkContext,publicScope} from '@/server/modules/identity_access/public/authorization';
import {commercePostingSetup} from '@/server/modules/commerce/public/posting-setup';
import {supplyPostingPrerequisites} from '@/server/modules/finance/public/supply-posting-prerequisites';
import {workspaceBase} from '@/lib/work_projection/workspace';
import {pageError} from '@/server/modules/work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),[initial,initialPrerequisites]=await Promise.all([commercePostingSetup(actor),supplyPostingPrerequisites(actor)]).catch(error=>pageError(error,holdingSlug));
 return <CommercePostingSetup base={workspaceBase(publicScope(actor))} initial={initial} initialPrerequisites={initialPrerequisites}/>;
}
