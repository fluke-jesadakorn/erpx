import { NextResponse } from 'next/server';
import { z } from 'zod';
import { requirePlatformContext, requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { parseHoldingWorkspace, workspaceBase, workspaceHref } from "@/lib/work_projection/workspace";
import { runtimeEnv } from "@/server/modules/runtime/public/runtime";
import { rememberWorkspace } from "@/server/modules/work_projection/public/workspace-preferences";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";

export async function POST(request:Request) {
  try {
    const {base}=z.object({base:z.string().max(512)}).parse(await request.json());
    const db=(await runtimeEnv()).CONTROL_DB;
    if(base==='/platform') {
      const actor=await requirePlatformContext();
      await rememberWorkspace(db,actor.userId,'/platform/home');
    } else {
      const parts=base.split('/').filter(Boolean);
      const parsed=parts[0]==='holdings' && parts[1] ? parseHoldingWorkspace(parts[1],parts.slice(2)) : null;
      if(!parsed || workspaceBase(parsed.scope)!==base)throw new ApiError(422,'INVALID_WORKSPACE','Choose an available workspace.');
      // The rendered session revision must still be current; an old tab cannot
      // overwrite the account preference after its role changes.
      if(!request.headers.has('x-erpx-context-revision'))throw new ApiError(409,'WORK_CONTEXT_CHANGED','Reload your workspace.');
      const actor=await requireWorkContext(parsed.scope);
      await rememberWorkspace(db,actor.userId,workspaceHref(publicScope(actor)),{holdingId:actor.holdingId,companyId:actor.companyId});
    }
    return NextResponse.json({data:{saved:true}},{headers:{'Cache-Control':'private, no-store'}});
  } catch(error) {return errorResponse(error);}
}
