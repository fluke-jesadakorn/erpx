import 'server-only';
import { DestinationIndexView } from "@erpx/workplace/components/destination-index-view";
import type { UiAppearance } from "@erpx/ui/components/appearance";
import { publicScope, requireWorkContext, requireManageErpxContext } from '@erpx/server-identity_access/public/authorization';
import { workspaceBase } from '@erpx/feature-work_projection/lib/workspace';
import { destinations, parentLabels, canonicalUiHref } from '@erpx/feature-work_projection/lib/destination-registry';
import { workspaceNavigation } from '@erpx/server-work_projection/public/navigation-snapshot';
import { workplaceNavigation } from '@erpx/feature-work_projection/lib/workplace-navigation';
import { administrationAccess } from '@erpx/server-runtime/public/administration';
import {companyFeatureCatalogue} from '@erpx/server-pms/public/hospitality-rollout';
import {availableNavigationAppIds} from '@erpx/feature-work_projection/lib/workplace-catalogue';
import {collaborationPresentationQueries} from '@erpx/server-collaboration/public/presentation-queries';
import {workProjectionPresentationQueries} from '@erpx/server-work_projection/public/presentation-queries';
import {financeWorkspaceV2Enabled} from '@erpx/server-finance/public/finance-presentation';
import { hasPermission } from '@erpx/feature-identity_access/lib/permissions';

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
