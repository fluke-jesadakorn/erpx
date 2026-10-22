import {ProjectPurchaseProvider} from '@erpx/feature-projects/components/purchase-context';
import {TaskEditor} from '@erpx/feature-work_projection/components/staff-work-controls';
import {RecordLink as Link} from '@erpx/feature-records/components/record-link';
import {FeaturePage} from '@erpx/feature-shared/components/features/feature-page';
import {InlineNotice} from '@erpx/ui/components/foundation';
import {canonicalUiHref} from '@erpx/feature-work_projection/lib/destination-registry';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {requireWorkContext,publicScope} from '@erpx/server-identity_access/public/authorization';
import {projectTaskCreationWorkspace} from '@erpx/server-projects/public/task-creation';
import {pageError} from '@erpx/server-work_projection/public/page-error';
export default async function NewProjectTask({params,searchParams}:{params:Promise<{holdingSlug:string;id:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor)),data=await projectTaskCreationWorkspace(actor,id).catch(error=>pageError(error,holdingSlug)),query=await searchParams;
 return <ProjectPurchaseProvider purchases={data.purchases}><FeaturePage className="ui-page-content" data-source="live" data-module="projects"><Link href={canonicalUiHref(`${base}/projects/${encodeURIComponent(data.project.id)}/tasks`)}>← {data.project.title}</Link><h1>Create a Project task</h1><p>Assign work within {data.project.title}. The task keeps this Project as its source.</p><ProjectPurchaseProvider purchases={data.purchases} editorSource={{companyId:actor.companyId,actorId:actor.userId,available:data.canCreate,record:null}}>{data.canCreate&&<TaskEditor appearance="standard" base={base} options={data.options} projects={[data.project]} userId={actor.userId} companyId={actor.companyId} assignmentId={actor.assignmentId} fixedProjectId={data.project.id} initialDueDate={typeof query.dueDate==='string'?query.dueDate:undefined}/>}</ProjectPurchaseProvider>{!data.canCreate&&<InlineNotice tone="info">{data.reason}</InlineNotice>}</FeaturePage></ProjectPurchaseProvider>;
}
