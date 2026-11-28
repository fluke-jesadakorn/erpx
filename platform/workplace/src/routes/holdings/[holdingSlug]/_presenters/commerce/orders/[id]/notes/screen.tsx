import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
import {requireWorkContext,publicScope} from "@erpx/identity-access-server/public/authorization";
import {workspaceBase} from "@erpx/work-projection-ui/lib/workspace";
import {pageError} from "@erpx/work-projection-server/public/page-error";
import {customerNotesForOrder} from "@erpx/sales-server/public/customer-notes";
import {CustomerNotes} from "@erpx/customer-360-ui/components/customer-notes";
import { PageContent } from "@erpx/platform-ui/components/foundation";
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){const {holdingSlug,id}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await customerNotesForOrder(a,id).catch(e=>pageError(e,holdingSlug));return <PageContent as="div" className="ui-page-content"><Link href={canonicalUiHref(`${base}/commerce/orders/${id}`)}>Back to source order</Link><h1>Customer contact notes</h1><p>{String(data.order.party_name)} · {String(data.order.number)}</p><p>Internal contact evidence for this order’s authorized team. These notes are not shared with the customer or printed on their documents. Record commitments in the approved order or contract before promising delivery.</p><CustomerNotes base={base} data={data}/></PageContent>;}
