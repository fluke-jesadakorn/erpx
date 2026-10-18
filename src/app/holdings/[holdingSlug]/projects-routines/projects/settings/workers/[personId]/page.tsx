import {ProjectWorkerStartup} from '@/components/projects/worker-startup';
import {projectWorkerStartupWorkspace,projectWorkerStartupAccounts,projectWorkerSelectionSchema} from '@/server/modules/people_payroll/public/project-worker-startup';
import {requireWorkContext,publicScope} from '@/server/modules/identity_access/public/authorization';
import {workspaceBase} from '@/lib/work_projection/workspace';
import {pageError} from '@/server/modules/work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;personId:string}>}){
 const {holdingSlug,personId}=await params,actor=await requireWorkContext(holdingSlug),selection=projectWorkerSelectionSchema.parse({personId});
 const initial=await projectWorkerStartupWorkspace(actor,selection).catch(error=>pageError(error,holdingSlug)),initialAccounts=initial.available?await projectWorkerStartupAccounts(actor,{}).catch(error=>pageError(error,holdingSlug)):{companyId:actor.companyId,accounts:[]};
 return <ProjectWorkerStartup base={workspaceBase(publicScope(actor))} initial={initial} initialAccounts={initialAccounts}/>;
}
