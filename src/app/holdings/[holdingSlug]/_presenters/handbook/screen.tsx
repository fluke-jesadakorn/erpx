import {redirect} from "next/navigation";
import {requireWorkContext,publicScope} from "@/server/modules/identity_access/public/authorization";
import {workspaceBase} from "@/lib/work_projection/workspace";
import {legacyWorkspaceDestination} from "@/lib/work_projection/section-workspaces";
import type {RouteSearch} from "@/lib/shared/route-context";
export default async function LegacyPage({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<RouteSearch>}){const actor=await requireWorkContext((await params).holdingSlug);redirect(legacyWorkspaceDestination(workspaceBase(publicScope(actor)),"handbook",await searchParams)!);}
