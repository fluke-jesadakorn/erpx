import 'server-only';
import {forbidden} from 'next/navigation';
import {iamPresentationQueries} from '@/server/modules/identity_access/public/presentation-context-queries';
import {requireWorkContext,publicScope} from '@/server/modules/identity_access/public/authorization';
import {workspaceBase} from '@/lib/work_projection/workspace';
import {hasPermission} from '@/lib/identity_access/permissions';
export async function technologyContext(holdingSlug:string,view:string){const local=await requireWorkContext(holdingSlug),{queries,actor}=await iamPresentationQueries(holdingSlug),base=workspaceBase(publicScope(local));const canReadDirectory=hasPermission(actor,'iam.read')&&!local.branchId&&!local.departmentId;if(view==='directory'&&!canReadDirectory||view!=='organization'&&!hasPermission(actor,'iam.read'))forbidden();return {local,actor,queries,base,canReadDirectory};}
