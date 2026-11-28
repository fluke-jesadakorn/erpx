import {manufacturingExistingPostingSetup} from '@erpx/commerce-server/public/manufacturing-existing-posting-setup';
import {hasPermission} from '@erpx/identity-access-ui/lib/permissions';
import {ManufacturingPostingSetup} from '@erpx/manufacturing-ui/components/posting-setup';
import {requireWorkContext,publicScope} from '@erpx/identity-access-server/public/authorization';
import {manufacturingPostingSetup} from '@erpx/manufacturing-server/public/posting-setup';
import {manufacturingPostingPrerequisites} from '@erpx/finance-server/public/manufacturing-posting-prerequisites';
import {manufacturingPostingProfiles} from '@erpx/manufacturing-server/public/posting-profiles';
import {workspaceBase} from '@erpx/work-projection-ui/lib/workspace';
import {pageError} from '@erpx/work-projection-server/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),[initial,initialPrerequisites,initialProfiles,initialExisting]=await Promise.all([manufacturingPostingSetup(actor),manufacturingPostingPrerequisites(actor),manufacturingPostingProfiles(actor),manufacturingExistingPostingSetup(actor)]).catch(error=>pageError(error,holdingSlug));
 return <ManufacturingPostingSetup base={workspaceBase(publicScope(actor))} initial={initial} initialPrerequisites={initialPrerequisites} initialProfiles={initialProfiles} initialExisting={initialExisting} canPrepareInitialStock={initial.available&&initial.configured===true&&!actor.departmentId&&hasPermission(actor,'inventory.read')&&hasPermission(actor,'inventory.prepare')}/>;
}
