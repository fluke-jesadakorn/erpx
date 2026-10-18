import 'server-only';
import { DestinationIndexView } from "./destination-index-view";
import type { UiAppearance } from "@/components/ui/appearance";
import { publicScope, requireWorkContext, requireManageErpxContext } from '@/server/modules/identity_access/public/authorization';
import { workspaceBase } from '@/lib/work_projection/workspace';
import { destinations, parentLabels, canonicalUiHref } from '@/lib/work_projection/destination-registry';
import { workspaceNavigation } from '@/server/modules/work_projection/public/navigation-snapshot';
import { workplaceNavigation } from '@/lib/work_projection/workplace-navigation';
import { administrationAccess } from '@/server/modules/runtime/public/administration';
import {companyFeatureCatalogue} from '@/server/modules/pms/public/hospitality-rollout';
import {availableNavigationAppIds} from '@/lib/work_projection/workplace-catalogue';
import {collaborationPresentationQueries} from '@/server/modules/collaboration/public/presentation-queries';
import {workProjectionPresentationQueries} from '@/server/modules/work_projection/public/presentation-queries';
import {financeWorkspaceV2Enabled} from '@/server/modules/finance/public/finance-presentation';
import { hasPermission } from '@/lib/identity_access/permissions';

export async function DestinationIndex({holdingSlug,path,appearance}:{holdingSlug:string;path:string;appearance:UiAppearance}) {
 const actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));
 if(path.startsWith('erpx-management'))await requireManageErpxContext();
 const snapshot=await workspaceNavigation(Promise.resolve({holdingSlug}));
 const [admin,catalogue,handbookAccess,executiveVisible]=await Promise.all([administrationAccess(actor),process.env.NODE_ENV==='production'?companyFeatureCatalogue(actor):Promise.resolve([]),(await collaborationPresentationQueries(actor.holdingId)).handbookAccess(actor).catch(()=>({editor:false,owner:false})),(await workProjectionPresentationQueries(actor.holdingId)).executiveVisible(actor)]);
 const model=workplaceNavigation({workspaceBase:base,handbookAccess,executiveVisible,financeWorkspaceV2:financeWorkspaceV2Enabled(),...(process.env.NODE_ENV==='production'?{availableAppIds:availableNavigationAppIds(catalogue)}:{}),scopeKind:actor.scope.kind,role:actor.role,moduleIds:actor.modules,permissions:actor.iam?.permissions,beta:actor.beta,adminSections:admin.sections,canManageApprovals:admin.manageOrganization,workViewAvailability:(await snapshot.initialNavigation)?.workViewAvailability,manageErpxVisible:holdingSlug==='erpx'&&hasPermission(actor,'manage_erpx.access')});
 const allowed=new Set(model.search.map(item=>canonicalUiHref(item.href).split('?')[0]));
 const candidates=destinations.filter(item=>item.path.startsWith(path+'/'));
 const visible=candidates.filter(item=>allowed.has(`${base}/${item.path}`));
 const children=new Map<string,string>();
 for(const item of visible){const child=item.path.slice(path.length+1).split('/')[0],next=`${path}/${child}`;children.set(next,parentLabels[next]??(next===item.path?item.label:child.replaceAll('-',' ')));}
 return <DestinationIndexView appearance={appearance} title={parentLabels[path]??path.split('/').at(-1)??'Workspace'} links={[...children].map(([next,label])=>({href:`${base}/${next}`,label}))}/>;
}
