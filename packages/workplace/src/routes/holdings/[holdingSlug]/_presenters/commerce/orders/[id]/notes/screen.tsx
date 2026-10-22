import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import {requireWorkContext,publicScope} from "@erpx/server-identity_access/public/authorization";
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
import {pageError} from "@erpx/server-work_projection/public/page-error";
import {customerNotesForOrder} from "@erpx/server-sales/public/customer-notes";
import {CustomerNotes} from "@erpx/feature-customer_360/components/customer-notes";
import { PageContent } from "@erpx/ui/components/foundation";
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){const {holdingSlug,id}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await customerNotesForOrder(a,id).catch(e=>pageError(e,holdingSlug));return <PageContent as="div" className="ui-page-content"><Link href={canonicalUiHref(`${base}/commerce/orders/${id}`)}>Back to source order</Link><h1>Customer contact notes</h1><p>{String(data.order.party_name)} · {String(data.order.number)}</p><p>Internal contact evidence for this order’s authorized team. These notes are not shared with the customer or printed on their documents. Record commitments in the approved order or contract before promising delivery.</p><CustomerNotes base={base} data={data}/></PageContent>;}
