import { notFound, redirect } from "next/navigation";
import { findModule, moduleHref } from "@/data/module-registry";
import { requirePlatformContext } from "@/server/authorization";

export default async function PlatformModuleLanding({ params }: PageProps<"/platform/modules/[module]">) {
  await requirePlatformContext();
  const { module: id } = await params;
  const definition = findModule(id);
  if (!definition) notFound();
  redirect(moduleHref("/platform", definition));
}
