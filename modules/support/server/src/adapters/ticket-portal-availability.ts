import 'server-only';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {supportTicketPortalWorkspace} from '@erpx/support-server/adapters/ticket-portal';
/** Optional link on an already-authorized Helpdesk detail. The dedicated
 * Portal workspace retains its strict purchase/release/record failures. */
export async function supportTicketPortalAvailable(actor:WorkContext,ticketId:string):Promise<boolean>{
 try{const view=await supportTicketPortalWorkspace(actor,{ticketId});return view.available&&view.canPrepare&&view.ticket?.id===ticketId;}
 catch(error){if(error instanceof ApiError&&(error.status===403||error.status===404||error.code==='SUPPORT_PORTAL_NOT_INSTALLED'))return false;throw error;}
}
