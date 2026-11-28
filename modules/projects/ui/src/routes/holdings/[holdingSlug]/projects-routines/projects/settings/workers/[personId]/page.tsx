import {ProjectWorkerStartup} from '@erpx/projects-ui/components/worker-startup';
import {projectWorkerStartupWorkspace,projectWorkerStartupAccounts,projectWorkerSelectionSchema} from '@erpx/people-payroll-server/public/project-worker-startup';
import {requireWorkContext,publicScope} from '@erpx/identity-access-server/public/authorization';
import {workspaceBase} from '@erpx/work-projection-ui/lib/workspace';
import {pageError} from '@erpx/work-projection-server/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;personId:string}>}){
 const {holdingSlug,personId}=await params,actor=await requireWorkContext(holdingSlug),selection=projectWorkerSelectionSchema.parse({personId});
 const initial=await projectWorkerStartupWorkspace(actor,selection).catch(error=>pageError(error,holdingSlug)),initialAccounts=initial.available?await projectWorkerStartupAccounts(actor,{}).catch(error=>pageError(error,holdingSlug)):{companyId:actor.companyId,accounts:[]};
 return <ProjectWorkerStartup base={workspaceBase(publicScope(actor))} initial={initial} initialAccounts={initialAccounts}/>;
}
