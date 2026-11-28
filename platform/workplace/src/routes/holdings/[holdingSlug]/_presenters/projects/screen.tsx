import WorkspaceLink from "@erpx/platform-workplace/components/workspace-link";
import {requireWorkContext} from "@erpx/identity-access-server/public/authorization";
import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";
import {projectWorkerStartupWorkspace} from "@erpx/people-payroll-server/public/project-worker-startup";
import {ProjectPurchaseProvider} from "@erpx/projects-ui/components/purchase-context";
import {projectPurchased} from "@erpx/projects-ui/lib/purchase-controls";
import {projectMemberPickerOptions} from "@erpx/projects-ui/lib/worker-display";
import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import {SectionWorkspace} from "@erpx/platform-workplace/components/section-workspace";
import {RecordLink as Link} from "@erpx/records-ui/components/record-link";
import {ListFilters} from "@erpx/platform-ui/components/list-filters";
import {ProjectEditor} from '@erpx/work-projection-ui/components/staff-work-controls';
import {staffPresentationQueries} from "@erpx/work-projection-server/public/presentation-context-queries";
import {routeContextHref,type RouteSearch} from "@erpx/platform-shared-ui/lib/route-context";
import {workspaceSearchContext} from "@erpx/work-projection-ui/lib/section-workspaces";
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
