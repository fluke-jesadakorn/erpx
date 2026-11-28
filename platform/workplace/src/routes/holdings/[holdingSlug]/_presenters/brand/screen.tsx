import {redirect} from "next/navigation";
import {requireWorkContext,publicScope} from "@erpx/identity-access-server/public/authorization";
import {workspaceBase} from "@erpx/work-projection-ui/lib/workspace";
import {legacyWorkspaceDestination} from "@erpx/work-projection-ui/lib/section-workspaces";
import type {RouteSearch} from "@erpx/platform-shared-ui/lib/route-context";
export default async function LegacyPage({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<RouteSearch>}){const actor=await requireWorkContext((await params).holdingSlug);redirect(legacyWorkspaceDestination(workspaceBase(publicScope(actor)),"brand",await searchParams)!);}
