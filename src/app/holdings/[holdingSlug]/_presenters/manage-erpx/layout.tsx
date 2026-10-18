import {headers} from 'next/headers';
import {productHostForHost} from '@/lib/runtime/product-hosts';
import {currentProviderContext} from '@/server/modules/identity_access/public/provider-context';
import {ProviderContextBoundary} from '@/components/identity_access/provider-context-boundary';
import {ProviderContextSwitcher} from '@/components/identity_access/provider-context-switcher';
import {ApiError} from '@/server/modules/runtime/public/http';
import {platformRoleDisplayChoices} from "@/lib/identity_access/work-role-display";
import { notFound } from "next/navigation";
import { requireManageErpxContext } from "@/server/modules/identity_access/public/authorization";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { PlatformRoleSwitcher } from "@/components/workplace/platform-role-switcher";
import { PlatformContextBoundary } from "@/components/workplace/platform-context-boundary";

export const dynamic = "force-dynamic";

export default async function ManageErpxLayout({
  children, params,
}: {
  children: React.ReactNode;
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params;
  if (holdingSlug !== "erpx") notFound();
  let context;
  try {
    context = await requireManageErpxContext();
  } catch (error) {
    return pageError(error, holdingSlug);
  }
  if(productHostForHost((await headers()).get('host'))==='admin'){
    const initial=await currentProviderContext();
    if(initial.selectedId!==context.platform.assignmentId)return pageError(new ApiError(409,'PLATFORM_CONTEXT_CHANGED','Your Platform context changed. Reload and review.'),holdingSlug);
    return <ProviderContextBoundary initial={initial}>
      <div data-platform-assignment={initial.selectedId}>
        <div className="content" style={{display:'flex',justifyContent:'flex-end',alignItems:'center',gap:16,flexWrap:'wrap'}}><ProviderContextSwitcher/></div>
        {children}
      </div>
    </ProviderContextBoundary>;
  }
  return <PlatformContextBoundary key={`${context.platform.userId}:${context.platform.assignmentId}`} userId={context.platform.userId} assignmentId={context.platform.assignmentId}>
    <div className="content" style={{ display: "flex", justifyContent: "flex-end", alignItems: "center", gap: 16, flexWrap: "wrap" }}>
      <PlatformRoleSwitcher activeAssignmentId={context.platform.assignmentId} assignments={platformRoleDisplayChoices(context.platform.assignments)} />
    </div>
    <div key={`${context.platform.userId}:${context.platform.assignmentId}`} data-platform-user={context.platform.userId}>{children}</div>
  </PlatformContextBoundary>;
}
