import {manufacturingExistingPostingSetup} from '@erpx/server-commerce/public/manufacturing-existing-posting-setup';
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import {ManufacturingPostingSetup} from '@erpx/feature-manufacturing/components/posting-setup';
import {requireWorkContext,publicScope} from '@erpx/server-identity_access/public/authorization';
import {manufacturingPostingSetup} from '@erpx/server-manufacturing/public/posting-setup';
import {manufacturingPostingPrerequisites} from '@erpx/server-finance/public/manufacturing-posting-prerequisites';
import {manufacturingPostingProfiles} from '@erpx/server-manufacturing/public/posting-profiles';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {pageError} from '@erpx/server-work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),[initial,initialPrerequisites,initialProfiles,initialExisting]=await Promise.all([manufacturingPostingSetup(actor),manufacturingPostingPrerequisites(actor),manufacturingPostingProfiles(actor),manufacturingExistingPostingSetup(actor)]).catch(error=>pageError(error,holdingSlug));
 return <ManufacturingPostingSetup base={workspaceBase(publicScope(actor))} initial={initial} initialPrerequisites={initialPrerequisites} initialProfiles={initialProfiles} initialExisting={initialExisting} canPrepareInitialStock={initial.available&&initial.configured===true&&!actor.departmentId&&hasPermission(actor,'inventory.read')&&hasPermission(actor,'inventory.prepare')}/>;
}
