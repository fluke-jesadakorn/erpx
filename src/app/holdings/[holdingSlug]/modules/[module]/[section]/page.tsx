import { notFound, redirect } from "next/navigation";
import { findModule, liveDestination } from "@/data/module-registry";
import { ModuleWorkspace } from "@/components/modules/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { hasModuleAccess } from "@/data/module-access";
import { listRecords } from "@/server/records";
export default async function ModuleSectionPage({
  params,
  searchParams,
}: PageProps<"/holdings/[holdingSlug]/modules/[module]/[section]">) {
  const { holdingSlug, module: id, section: sectionId } = await params;
  const context = await requireWorkContext(holdingSlug);
  const base = workspaceBase(publicScope(context));
  const search = await searchParams;
  const definition = findModule(id);
  const section = definition?.sections.find((item) => item.id === sectionId);
  if (!definition || !section) notFound();
  const destination = liveDestination(id, sectionId);
  if (destination) redirect(`${base}/${destination}`);
  const liveRecords = hasModuleAccess(context.modules, id)
    ? await listRecords(context, id, sectionId)
    : [];
  return (
    <ModuleWorkspace
      workspaceBase={base}
      module={definition}
      section={section}
      liveRecords={liveRecords}
      canCreate={hasModuleAccess(context.modules, id) && id !== "finance"}
      query={{
        q: typeof search.q === "string" ? search.q : undefined,
        status: typeof search.status === "string" ? search.status : undefined,
        sort: typeof search.sort === "string" ? search.sort : undefined,
      }}
    />
  );
}
