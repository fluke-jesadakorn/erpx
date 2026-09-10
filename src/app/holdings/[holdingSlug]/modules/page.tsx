import { Suspense } from "react";
import { ModuleDirectory } from "@/components/modules/directory";
import { Skeleton } from "@/components/ui/primitives";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
export default async function ModulesPage({
  params,
}: PageProps<"/holdings/[holdingSlug]/modules">) {
  const { holdingSlug } = await params;
  const context = await requireWorkContext(holdingSlug);
  return (
    <Suspense fallback={<Skeleton />}>
      <ModuleDirectory
        workspaceBase={workspaceBase(publicScope(context))}
        allowedModules={context.modules}
      />
    </Suspense>
  );
}
