import {RosterOwnerWorkspace} from '@erpx/people-payroll-ui/components/roster-owner-workspace';
import {rosterPresentation} from '@erpx/people-payroll-server/public/roster-presentation';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<Record<string,string|undefined>>}){const query=await searchParams,{base,data}=await rosterPresentation((await params).holdingSlug,query);return <RosterOwnerWorkspace appearance="standard" initial={data} base={base} initialView={query.view} requestId={query.request}/>;}
