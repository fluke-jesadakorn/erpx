import 'server-only';
import {forbidden} from 'next/navigation';
import {iamPresentationQueries} from '@erpx/identity-access-server/public/presentation-context-queries';
import {requireWorkContext,publicScope} from '@erpx/identity-access-server/public/authorization';
import {workspaceBase} from '@erpx/work-projection-ui/lib/workspace';
import {hasPermission} from '@erpx/identity-access-ui/lib/permissions';
export async function technologyContext(holdingSlug:string,view:string){const local=await requireWorkContext(holdingSlug),{queries,actor}=await iamPresentationQueries(holdingSlug),base=workspaceBase(publicScope(local));const canReadDirectory=hasPermission(actor,'iam.read')&&!local.branchId&&!local.departmentId;if(view==='directory'&&!canReadDirectory||view!=='organization'&&!hasPermission(actor,'iam.read'))forbidden();return {local,actor,queries,base,canReadDirectory};}
