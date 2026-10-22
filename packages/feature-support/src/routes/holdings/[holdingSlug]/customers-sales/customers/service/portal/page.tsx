import {SupportTicketPortalSetup} from '@erpx/feature-support/components/ticket-portal';
import {supportTicketPortalWorkspace,supportTicketPortalSelectionSchema} from '@erpx/server-support/public/ticket-portal';
import {requireWorkContext,publicScope} from '@erpx/server-identity_access/public/authorization';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {pageError} from '@erpx/server-work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{ticketId?:string|string[]}>}){
 const {holdingSlug}=await params,query=await searchParams,actor=await requireWorkContext(holdingSlug),selection=supportTicketPortalSelectionSchema.parse(query.ticketId===undefined?{}:{ticketId:query.ticketId}),initial=await supportTicketPortalWorkspace(actor,selection).catch(error=>pageError(error,holdingSlug));return <SupportTicketPortalSetup base={workspaceBase(publicScope(actor))} initial={initial}/>;
}
