import { notFound } from "next/navigation";
import { ModuleWorkspace } from "@/components/modules/workspace";
import { findModule } from "@/data/module-registry";
import { requirePlatformContext } from "@/server/authorization";

export default async function PlatformModulePage({ params, searchParams }: PageProps<"/platform/modules/[module]/[section]">) {
  await requirePlatformContext();
  const { module: id, section: sectionId } = await params;
  const search = await searchParams;
  const definition = findModule(id);
  const section = definition?.sections.find((item) => item.id === sectionId);
  if (!definition || !section) notFound();
  return <ModuleWorkspace workspaceBase="/platform" module={definition} section={section} query={{
    q: typeof search.q === "string" ? search.q : undefined,
    status: typeof search.status === "string" ? search.status : undefined,
    sort: typeof search.sort === "string" ? search.sort : undefined,
  }} />;
}
