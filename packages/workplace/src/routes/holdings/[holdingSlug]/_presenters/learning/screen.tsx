import {redirect} from "next/navigation";
import {requireWorkContext,publicScope} from "@erpx/server-identity_access/public/authorization";
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
import {legacyWorkspaceDestination} from "@erpx/feature-work_projection/lib/section-workspaces";
import type {RouteSearch} from "@erpx/feature-shared/lib/route-context";
export default async function LegacyPage({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<RouteSearch>}){const actor=await requireWorkContext((await params).holdingSlug);redirect(legacyWorkspaceDestination(workspaceBase(publicScope(actor)),"learning",await searchParams)!);}
