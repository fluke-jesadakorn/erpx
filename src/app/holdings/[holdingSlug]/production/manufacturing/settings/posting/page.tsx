import {manufacturingExistingPostingSetup} from '@/server/modules/commerce/public/manufacturing-existing-posting-setup';
import {hasPermission} from '@/lib/identity_access/permissions';
import {ManufacturingPostingSetup} from '@/components/manufacturing/posting-setup';
import {requireWorkContext,publicScope} from '@/server/modules/identity_access/public/authorization';
import {manufacturingPostingSetup} from '@/server/modules/manufacturing/public/posting-setup';
import {manufacturingPostingPrerequisites} from '@/server/modules/finance/public/manufacturing-posting-prerequisites';
import {manufacturingPostingProfiles} from '@/server/modules/manufacturing/public/posting-profiles';
import {workspaceBase} from '@/lib/work_projection/workspace';
import {pageError} from '@/server/modules/work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),[initial,initialPrerequisites,initialProfiles,initialExisting]=await Promise.all([manufacturingPostingSetup(actor),manufacturingPostingPrerequisites(actor),manufacturingPostingProfiles(actor),manufacturingExistingPostingSetup(actor)]).catch(error=>pageError(error,holdingSlug));
 return <ManufacturingPostingSetup base={workspaceBase(publicScope(actor))} initial={initial} initialPrerequisites={initialPrerequisites} initialProfiles={initialProfiles} initialExisting={initialExisting} canPrepareInitialStock={initial.available&&initial.configured===true&&!actor.departmentId&&hasPermission(actor,'inventory.read')&&hasPermission(actor,'inventory.prepare')}/>;
}
