import {SupportTicketPortal} from '@/components/customer_authority/support-ticket-portal';
import {supportTicketPortalProjectionRequest} from '@/server/modules/support/public/ticket-portal';
import {redirect} from 'next/navigation';
import {ApiError} from '@/server/modules/runtime/public/http';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,initial=await supportTicketPortalProjectionRequest(holdingSlug).catch(error=>{if(error instanceof Error&&error.message==='UNAUTHENTICATED'||error instanceof ApiError&&error.status===401)redirect('/login?'+new URLSearchParams({next:`/customer-portal/${encodeURIComponent(holdingSlug)}/tickets`}));throw error;});return <SupportTicketPortal holdingSlug={holdingSlug} initial={initial}/>;
}
