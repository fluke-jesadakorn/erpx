import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import {requireWorkContext,publicScope} from "@erpx/server-identity_access/public/authorization";
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
import {pageError} from "@erpx/server-work_projection/public/page-error";
import {supportSlaWorkspace} from "@erpx/server-support/public/support-sla";
import {SupportSlaConsole} from "@erpx/feature-support/components/support-sla-console";
import { PageContent, Card } from "@erpx/ui/components/foundation";
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){const {holdingSlug}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await supportSlaWorkspace(a).catch(e=>pageError(e,holdingSlug));return <PageContent as="div" className="ui-page-content"><Link href={canonicalUiHref(`${base}/support`)}>Support cases</Link><h1>Resolution-time watch</h1><p>Track approved working-time targets. Resolving a case stops this clock; customer confirmation remains a separate step. Reopening uses the remaining budget.</p><Card as="section" appearance="standard" className="ui-panel"><h2>Cases with a target</h2>{data.items.map(r=>{const c=r.clock,breached=c.breachedAt!==null||(c.state==='running'&&c.dueAt!==null&&c.dueAt<data.now);return <Link className="ui-record-item" key={String(r.id)} href={canonicalUiHref(`${base}/support/${r.id}`)}><strong>{String(r.title)} · {breached?'Target breached':c.state}</strong><span>{c.dueAt?<>Target <TimeText appearance="standard" value={c.dueAt} /></>:c.state==='paused'?'Clock paused':'Clock stopped'} · {String(r.status)}</span></Link>;})}{!data.items.length&&<p>No active cases with an attached target.</p>}</Card><SupportSlaConsole base={base} data={data}/></PageContent>;}
