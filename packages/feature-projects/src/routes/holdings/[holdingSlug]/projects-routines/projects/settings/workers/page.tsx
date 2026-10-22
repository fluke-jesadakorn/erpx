import {ProjectWorkerStartup} from '@erpx/feature-projects/components/worker-startup';
import {projectWorkerStartupWorkspace,projectWorkerStartupAccounts,projectWorkerSelectionSchema} from '@erpx/server-people_payroll/public/project-worker-startup';
import {requireWorkContext,publicScope} from '@erpx/server-identity_access/public/authorization';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {pageError} from '@erpx/server-work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){
 const {holdingSlug}=await params,query=await searchParams,actor=await requireWorkContext(holdingSlug),selection=projectWorkerSelectionSchema.parse({...(query.accountId===undefined?{}:{accountId:query.accountId}),...(query.personId===undefined?{}:{personId:query.personId})});
 const initial=await projectWorkerStartupWorkspace(actor,selection).catch(error=>pageError(error,holdingSlug)),initialAccounts=initial.available?await projectWorkerStartupAccounts(actor,{}).catch(error=>pageError(error,holdingSlug)):{companyId:actor.companyId,accounts:[]};
 return <ProjectWorkerStartup base={workspaceBase(publicScope(actor))} initial={initial} initialAccounts={initialAccounts}/>;
}
