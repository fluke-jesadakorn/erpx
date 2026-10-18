import {SupportTicketPortalSetup} from '@/components/support/ticket-portal';
import {supportTicketPortalWorkspace,supportTicketPortalSelectionSchema} from '@/server/modules/support/public/ticket-portal';
import {requireWorkContext,publicScope} from '@/server/modules/identity_access/public/authorization';
import {workspaceBase} from '@/lib/work_projection/workspace';
import {pageError} from '@/server/modules/work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;bindingId:string}>}){
 const {holdingSlug,bindingId}=await params,actor=await requireWorkContext(holdingSlug),selection=supportTicketPortalSelectionSchema.parse({bindingId}),initial=await supportTicketPortalWorkspace(actor,selection).catch(error=>pageError(error,holdingSlug));return <SupportTicketPortalSetup base={workspaceBase(publicScope(actor))} initial={initial} bindingId={bindingId}/>;
}
