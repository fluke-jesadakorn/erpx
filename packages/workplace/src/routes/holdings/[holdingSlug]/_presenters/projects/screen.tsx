import WorkspaceLink from "@erpx/workplace/components/workspace-link";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {projectWorkerStartupWorkspace} from "@erpx/server-people_payroll/public/project-worker-startup";
import {ProjectPurchaseProvider} from "@erpx/feature-projects/components/purchase-context";
import {projectPurchased} from "@erpx/feature-projects/lib/purchase-controls";
import {projectMemberPickerOptions} from "@erpx/feature-projects/lib/worker-display";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import {SectionWorkspace} from "@erpx/workplace/components/section-workspace";
import {RecordLink as Link} from "@erpx/feature-records/components/record-link";
import {ListFilters} from "@erpx/ui/components/list-filters";
import {ProjectEditor} from '@erpx/feature-work_projection/components/staff-work-controls';
import {staffPresentationQueries} from "@erpx/server-work_projection/public/presentation-context-queries";
import {routeContextHref,type RouteSearch} from "@erpx/feature-shared/lib/route-context";
import {workspaceSearchContext} from "@erpx/feature-work_projection/lib/section-workspaces";
export default async function Projects({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<RouteSearch>}){
 const {holdingSlug}=await params,{actor,base,options,projects,staff,queries}=await staffPresentationQueries(holdingSlug),query=await searchParams,term=typeof query.q==='string'?query.q:'';
 const purchases=await queries.projectPurchases();
 let canPrepareWorker=false;
 if(!staff){
  const current=await requireWorkContext(holdingSlug);
  if(current.companyId===actor.companyId&&current.holdingId===actor.holdingId&&!current.beta&&current.scope.kind==='holding'&&!current.branchId&&!current.departmentId&&hasPermission(current,'people.manage')&&hasPermission(current,'organization.manage')){
   const preparation=await projectWorkerStartupWorkspace(current,{});
   canPrepareWorker=preparation.available&&preparation.companyId===current.companyId;
  }
 }
 const visible=projects.filter(project=>!term||`${project.title} ${project.description}`.toLocaleLowerCase().includes(term.toLocaleLowerCase()));
 return <ProjectPurchaseProvider purchases={purchases}><SectionWorkspace title="Projects" selected="all" items={[{id:'all',title:'All projects',href:routeContextHref(`${base}/projects`,query,{view:null,detailKind:null,detailId:null})}]} context={`${visible.length} projects in your scope`}>
 <ListFilters appearance="standard" action={canonicalUiHref(`${base}/projects`)} query={term} preserve={workspaceSearchContext(query)} placeholder="Search projects"/><ProjectPurchaseProvider purchases={purchases} editorSource={{companyId:actor.companyId,actorId:actor.userId,available:Boolean(staff),record:null}}>{staff&&projectPurchased(purchases,"projects.wbs","newWork")&&<ProjectEditor appearance="standard" base={base} commandOwner={{companyId:actor.companyId,actorId:actor.userId}} options={projectMemberPickerOptions(options)}/>}</ProjectPurchaseProvider>
 {canPrepareWorker&&<p>Your Company account needs an active worker profile before it can create or join Project work. <WorkspaceLink href={`${base}/projects-routines/projects/settings/workers`}>Prepare a Project worker</WorkspaceLink></p>}
 <div className="ui-stack">{visible.map(project=><article className="ui-record-item" data-selected={query.detailId===project.id||undefined} key={project.id}><h2><Link href={canonicalUiHref(`${base}/projects/${project.id}`)}>{project.title}</Link></h2><p>{project.description}</p></article>)}{!visible.length&&<p>No projects match this view.</p>}</div>
 </SectionWorkspace></ProjectPurchaseProvider>;
}
