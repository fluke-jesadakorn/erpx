import { Suspense } from "react";
import { ModuleDirectory } from "@/components/modules/directory";
import { Skeleton } from "@/components/ui/primitives";
import { requirePlatformContext } from "@/server/authorization";

export default async function PlatformModulesPage() {
  const context = await requirePlatformContext();
  return <Suspense fallback={<Skeleton />}>
    <ModuleDirectory workspaceBase="/platform" allowedModules={context.modules} />
  </Suspense>;
}
