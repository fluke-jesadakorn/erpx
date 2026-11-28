import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { TimeText } from "@erpx/people-payroll-ui/components/time/time-preference-context";
import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
import {requireWorkContext,publicScope} from "@erpx/identity-access-server/public/authorization";
import {workspaceBase} from "@erpx/work-projection-ui/lib/workspace";
import {pageError} from "@erpx/work-projection-server/public/page-error";
import {createNativeSupportReader} from "@erpx/support-ui/lib/native-support-read";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {headers} from "next/headers";
import {SupportSlaConsole} from "@erpx/support-ui/components/support-sla-console";
import { PageContent, Card } from "@erpx/platform-ui/components/foundation";
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){const {holdingSlug}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),config=process.env.ERPX_NATIVE_SUPPORT_READ_CONFIG;if(!config)throw new ApiError(503,'NATIVE_SUPPORT_READ_UNAVAILABLE','The native Support reader is unavailable.');const data=await createNativeSupportReader(config).sla(await headers(),holdingSlug).catch(e=>pageError(e,holdingSlug));return <PageContent as="div" className="ui-page-content"><Link href={canonicalUiHref(`${base}/support`)}>Support cases</Link><h1>Resolution-time watch</h1><p>Track approved working-time targets. Resolving a case stops this clock; customer confirmation remains a separate step. Reopening uses the remaining budget.</p><Card as="section" appearance="standard" className="ui-panel"><h2>Cases with a target</h2>{data.items.map(r=>{const c=r.clock,breached=c.breachedAt!==null||(c.state==='running'&&c.dueAt!==null&&c.dueAt<data.now);return <Link className="ui-record-item" key={String(r.id)} href={canonicalUiHref(`${base}/support/${r.id}`)}><strong>{String(r.title)} · {breached?'Target breached':c.state}</strong><span>{c.dueAt?<>Target <TimeText appearance="standard" value={c.dueAt} /></>:c.state==='paused'?'Clock paused':'Clock stopped'} · {String(r.status)}</span></Link>;})}{!data.items.length&&<p>No active cases with an attached target.</p>}</Card><SupportSlaConsole base={base} data={data}/></PageContent>;}
