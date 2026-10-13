import { RecordLink as Link } from "@/components/records/record-link";
import {requireWorkContext,publicScope} from "@/server/modules/identity_access/public/authorization";
import {workspaceBase} from "@/lib/work_projection/workspace";
import {pageError} from "@/server/modules/work_projection/public/page-error";
import {customerNotesForOrder} from "@/server/modules/sales/public/customer-notes";
import {CustomerNotes} from "@/components/customer_360/customer-notes";
import { PageContent } from "@/components/ui/foundation";
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){const {holdingSlug,id}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await customerNotesForOrder(a,id).catch(e=>pageError(e,holdingSlug));return <PageContent as="div" className="ui-page-content"><Link href={`${base}/commerce/orders/${id}`}>Back to source order</Link><h1>Customer contact notes</h1><p>{String(data.order.party_name)} · {String(data.order.number)}</p><p>Internal contact evidence for this order’s authorized team. These notes are not shared with the customer or printed on their documents. Record commitments in the approved order or contract before promising delivery.</p><CustomerNotes base={base} data={data}/></PageContent>;}
