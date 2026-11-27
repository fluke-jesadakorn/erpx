import {headers} from "next/headers";
import {notFound} from "next/navigation";
import {holdingOrigin, holdingUiUrl} from "@erpx/feature-runtime/lib/holding-hosts";
import { SocialPage } from "@erpx/feature-collaboration/components/social-page";
import { oasisDemoStagingEnabled } from "@erpx/contracts/holding-hosts";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { OasisFeed } from "./oasis-feed";
export default async function Home({params,searchParams}:{ params: Promise<{ holdingSlug: string;  }>; searchParams: Promise<Record<string,string|string[]|undefined>> }) {
  const {holdingSlug}=await params;
  const actor = await requireWorkContext(holdingSlug);
  const incoming=await headers();
  const pluginId=incoming.get("x-erpx-plugin-id");
  if(pluginId){
    const scope=publicScope(actor);
    const company=incoming.get("x-erpx-route-company");
    const origin=holdingOrigin(holdingSlug);
    if(!origin||!company)notFound();
    const workspace=workspaceBase(scope);
    const result=await fetch(`${origin}/__erpx/apps/api/v1${workspace}/plugins?catalogue=0`,{
      headers:{cookie:incoming.get("cookie")??"",origin,"x-erpx-requested-company":company},
      cache:"no-store",signal:AbortSignal.timeout(5000),
    });
    if([401,403,404,409].includes(result.status))notFound();
    if(!result.ok)throw new Error("PLUGIN_CATALOGUE_UNAVAILABLE");
    const {data}=await result.json() as {data?:{installed:{id:string;manifest:{name:string}}[];siteMeasurement?:{subject:string;dueDate:string;leadId:string}}};
    const installed=data?.installed.find(item=>item.id===pluginId);
    if(!installed)notFound();
    const measurement=data?.siteMeasurement;
    const crm=measurement&&holdingUiUrl(`${workspace}/customers-sales/sales/pipeline/${encodeURIComponent(measurement.leadId)}/overview#crm-activities`,company);
    return <main><h1>{installed.manifest.name}</h1>{measurement
      ? <><p>{measurement.subject} · <time dateTime={measurement.dueDate}>{measurement.dueDate}</time></p>{crm&&<a href={crm}>Open in CRM</a>}</>
      : <p>No upcoming site measurements</p>}</main>;
  }
  if (oasisDemoStagingEnabled(process.env)) {
    return <OasisFeed api={`/api/v1${workspaceBase(publicScope(actor))}`} />;
  }
  const query = await searchParams;
  return <SocialPage actor={actor} tag={typeof query.tag === "string" ? query.tag.normalize("NFC").toLowerCase() : undefined}/>;
}
