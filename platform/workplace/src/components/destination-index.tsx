import { oasisDemoStagingEnabled } from '@erpx/platform-contracts/holding-hosts';
import 'server-only';
import { DestinationIndexView } from "@erpx/platform-workplace/components/destination-index-view";
import type { UiAppearance } from "@erpx/platform-ui/components/appearance";
import { publicScope, requireWorkContext, requireManageErpxContext } from '@erpx/identity-access-server/public/authorization';
import { workspaceBase } from '@erpx/work-projection-ui/lib/workspace';
import { destinations, parentLabels, canonicalUiHref } from '@erpx/work-projection-ui/lib/destination-registry';
import { workspaceNavigation } from '@erpx/work-projection-server/public/navigation-snapshot';
import { workplaceNavigation } from '@erpx/work-projection-ui/lib/workplace-navigation';
import { administrationAccess } from '@erpx/platform-runtime-server/public/administration';
import {companyFeatureCatalogue} from '@erpx/pms-server/public/hospitality-rollout';
import {availableNavigationAppIds} from '@erpx/work-projection-ui/lib/workplace-catalogue';
import {collaborationPresentationQueries} from '@erpx/collaboration-server/public/presentation-queries';
import {workProjectionPresentationQueries} from '@erpx/work-projection-server/public/presentation-queries';
import {financeWorkspaceV2Enabled} from '@erpx/finance-server/public/finance-presentation';
import { hasPermission } from '@erpx/identity-access-ui/lib/permissions';
import { redirect } from 'next/navigation';
import { navigationWorkEntry, workEntries, navigationTopicHref } from '@erpx/work-projection-ui/lib/navigation-presentation';
import { routeContextHref, routeSearchFromParams, type RouteSearch } from '@erpx/platform-shared-ui/lib/route-context';

export async function DestinationIndex({holdingSlug,path,appearance,openWork=false,searchParams}:{holdingSlug:string;path:string;appearance:UiAppearance;openWork?:boolean;searchParams?:Promise<RouteSearch>}) {
 const actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));
 if(path.startsWith('erpx-management'))await requireManageErpxContext();
 const snapshot=await workspaceNavigation(Promise.resolve({holdingSlug}));
 const oasisDemo = oasisDemoStagingEnabled(process.env);
 const [admin,catalogue,handbookAccess,executiveVisible]=await Promise.all([administrationAccess(actor),process.env.NODE_ENV==='production'&&!oasisDemo?companyFeatureCatalogue(actor):Promise.resolve([]),(await collaborationPresentationQueries(actor.holdingId)).handbookAccess(actor).catch(()=>({editor:false,owner:false})),(await workProjectionPresentationQueries(actor.holdingId)).executiveVisible(actor)]);
 const model=workplaceNavigation({workspaceBase:base,handbookAccess,executiveVisible,financeWorkspaceV2:financeWorkspaceV2Enabled(),...(process.env.NODE_ENV==='production'?{availableAppIds:oasisDemo?['apps','sales','crm']:availableNavigationAppIds(catalogue)}:{}),scopeKind:actor.scope.kind,role:actor.role,moduleIds:actor.modules,permissions:actor.iam?.permissions,beta:actor.beta,adminSections:admin.sections,canManageApprovals:admin.manageOrganization,workViewAvailability:(await snapshot.initialNavigation)?.workViewAvailability,manageErpxVisible:holdingSlug==='erpx'&&hasPermission(actor,'manage_erpx.access')});
 const root=navigationWorkEntry(model,base,`${base}/${path}`),definition=workEntries.find(entry=>entry.id===root?.id);
 const daily=model.primary.find(entry=>entry.id===path);
 const landing=root?.href??(daily?.id==='calendar'?daily.submodules?.find(item=>item.href.endsWith('/calendar/team'))?.href:undefined)??daily?.submodules?.[0]?.href;
 const search=await searchParams??{},query=new URLSearchParams();
 for(const [key,value] of Object.entries(search))for(const item of Array.isArray(value)?value:value?[value]:[])query.append(key,item);
 if(openWork&&landing&&definition?.mode!=='index'&&landing.split('?')[0]!==`${base}/${path}`){const target=new URL(landing,'https://navigation.invalid');redirect(routeContextHref(target.pathname,{...routeSearchFromParams(target.searchParams),...search}));}
 const category=model.categories.find(entry=>entry.id===path);
 if(root||daily||category){const items=category?.apps??root?.submodules??daily?.submodules??[];return <DestinationIndexView appearance={appearance} title={root?.label??daily?.label??category?.label??'Workspace'} links={items.map(item=>({href:navigationTopicHref(item.href,query),label:item.label,description:item.keywords?.[0]}))}/>;}
 const allowed=new Set(model.search.map(item=>canonicalUiHref(item.href).split('?')[0]));
 const candidates=destinations.filter(item=>item.path.startsWith(path+'/'));
 const visible=candidates.filter(item=>allowed.has(`${base}/${item.path}`));
 const children=new Map<string,string>();
 for(const item of visible){const child=item.path.slice(path.length+1).split('/')[0],next=`${path}/${child}`;children.set(next,parentLabels[next]??(next===item.path?item.label:child.replaceAll('-',' ')));}
 return <DestinationIndexView appearance={appearance} title={parentLabels[path]??path.split('/').at(-1)??'Workspace'} links={[...children].map(([next,label])=>({href:`${base}/${next}`,label}))}/>;
}
