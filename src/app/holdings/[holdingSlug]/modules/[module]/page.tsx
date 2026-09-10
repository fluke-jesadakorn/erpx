import { notFound, redirect } from "next/navigation";
import { findModule, moduleHref } from "@/data/module-registry";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
export default async function ModuleLanding({
  params,
}: PageProps<"/holdings/[holdingSlug]/modules/[module]">) {
  const { holdingSlug, module: id } = await params;
  const context = await requireWorkContext(holdingSlug);
  const definition = findModule(id);
  if (!definition) notFound();
  redirect(moduleHref(workspaceBase(publicScope(context)), definition));
}
