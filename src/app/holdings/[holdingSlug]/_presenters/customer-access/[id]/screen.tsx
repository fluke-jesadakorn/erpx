import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import { RecordLink as Link } from "@/components/records/record-link";
import {requireWorkContext,publicScope} from "@/server/modules/identity_access/public/authorization";
import {workspaceBase} from "@/lib/work_projection/workspace";
import {pageError} from "@/server/modules/work_projection/public/page-error";
import {customerAccessHistory} from "@/server/modules/customer_authority/public/customer-access";
import { CustomerAccessConsole } from "@/components/records/loaders/modules--customer-access-console";
import { PageContent, Card } from "@/components/ui/foundation";
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){const {holdingSlug,id}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await customerAccessHistory(a,id).catch(e=>pageError(e,holdingSlug)),r=data.binding;return <PageContent as="div" className="ui-page-content"><Link href={canonicalUiHref(`${base}/customer-access`)}>Customer access</Link><h1>{String(r.number)}</h1><CustomerAccessConsole key={`${r.id}:${r.version}`} base={base} detail={data}/><Card as="section" appearance="standard" className="ui-panel"><h2>Immutable decision history</h2>{data.events.map(e=><p key={`${e.version}`}>Version {String(e.version)} · {String(e.action)} · {String(e.reason)}</p>)}</Card><Card as="section" appearance="standard" className="ui-panel"><h2>Customer read audit</h2>{data.reads.map((e,i)=><p key={`${e.read_at}:${i}`}>{<TimeText appearance="standard" value={Number(e.read_at)} />} · binding version {String(e.binding_version)} · projection {String(e.projection_version)}</p>)}{!data.reads.length&&<p>No customer reads recorded.</p>}</Card></PageContent>;}
