import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { InlineNotice } from "@erpx/platform-ui/components/foundation";
import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
import {requireWorkContext,publicScope} from "@erpx/identity-access-server/public/authorization";
import {workspaceBase} from "@erpx/work-projection-ui/lib/workspace";
import {pageError} from "@erpx/work-projection-server/public/page-error";
import {customerFollowupDetail} from "@erpx/sales-server/public/customer-followups";
import { CustomerFollowupForm } from "@erpx/records-ui/components/loaders/modules--customer-followups";
import { PageContent, Card } from "@erpx/platform-ui/components/foundation";
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){const {holdingSlug,id}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await customerFollowupDetail(a,id).catch(e=>pageError(e,holdingSlug)),r=data.row;return <PageContent as="div" className="ui-page-content"><Link href={canonicalUiHref(`${base}/customer-followups`)}>All follow-ups</Link><h1>{String(r.customer_name)} · Follow-up</h1><Card as="section" appearance="standard" className="ui-panel"><p>{String(r.next_action)}</p><p>{String(r.status)} · Due {String(r.due_date)} · Owner {String(r.owner_name??'Assigned sales colleague')}</p><Link href={canonicalUiHref(`${base}/commerce/orders/${r.order_id}`)}>Source order · {String(r.order_number)}</Link>{!Number(r.owner_eligible)&&r.status==='open'&&<InlineNotice tone="danger" role="alert">Owner is no longer eligible. A sales manager must reassign with a reason.</InlineNotice>}{r.outcome&&<p>Outcome: {String(r.outcome)}</p>}</Card><CustomerFollowupForm key={String(r.id)} base={base} detail={data} recipients={data.recipients}/><Card as="section" appearance="standard" className="ui-panel"><h2>Handoff history</h2>{data.history.map(e=><p key={String(e.id)}>Version {String(e.version)} · {String(e.action)} · {String(e.reason)} · Owner {String(e.owner_id)} · Due {String(e.due_date)}</p>)}</Card></PageContent>;}
