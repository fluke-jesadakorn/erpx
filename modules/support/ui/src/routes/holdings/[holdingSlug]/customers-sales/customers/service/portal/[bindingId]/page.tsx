import {SupportTicketPortalSetup} from '@erpx/support-ui/components/ticket-portal';
import {supportTicketPortalWorkspace,supportTicketPortalSelectionSchema} from '@erpx/support-server/public/ticket-portal';
import {requireWorkContext,publicScope} from '@erpx/identity-access-server/public/authorization';
import {workspaceBase} from '@erpx/work-projection-ui/lib/workspace';
import {pageError} from '@erpx/work-projection-server/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;bindingId:string}>}){
 const {holdingSlug,bindingId}=await params,actor=await requireWorkContext(holdingSlug),selection=supportTicketPortalSelectionSchema.parse({bindingId}),initial=await supportTicketPortalWorkspace(actor,selection).catch(error=>pageError(error,holdingSlug));return <SupportTicketPortalSetup base={workspaceBase(publicScope(actor))} initial={initial} bindingId={bindingId}/>;
}
