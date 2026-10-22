import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import {requireWorkContext,publicScope} from "@erpx/server-identity_access/public/authorization";
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
import {pageError} from "@erpx/server-work_projection/public/page-error";
import {operationsWorkspace} from "@erpx/server-operations/public/operations-exceptions";
import {OperationsConsole} from "@erpx/feature-operations/components/operations-console";
import { PageContent, Card } from "@erpx/ui/components/foundation";
export const dynamic="force-dynamic";
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await operationsWorkspace(a).catch(e=>pageError(e,holdingSlug));
 return <PageContent as="div" className="ui-page-content"><h1>Operations</h1><p>Exceptions from authorized source records, accountable owners and reviewed resolutions.</p><Link href={canonicalUiHref(`${base}/staff-tasks`)}>Tasks and recurring work</Link><Card as="section" appearance="standard" className="ui-panel"><h2>Current queue</h2><p>{data.summary.active} active · {data.summary.awaitingReview} awaiting review · {data.summary.escalated} escalated · {data.summary.overdue} past target</p>{Object.entries(data.summary.byArea).map(([area,count])=><p key={area}>{area} · {count}</p>)}</Card><OperationsConsole base={base} data={data}/><Card as="section" appearance="standard" className="ui-panel"><h2>Exceptions</h2>{data.exceptions.length===0&&<p>No exceptions in your current source scope.</p>}{data.exceptions.map(row=><Link className="ui-record-item" key={row.id} href={canonicalUiHref(`${base}/operations/${row.id}`)}>{row.title} · {row.status} · {row.priority} · {row.source.label}</Link>)}</Card></PageContent>;
}
