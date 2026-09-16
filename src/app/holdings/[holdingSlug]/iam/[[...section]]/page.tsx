import { notFound } from "next/navigation";
import { IamConsole } from "@/components/iam/iam-console";
import { iamSections, type IamSection } from "@/lib/iam";
import { iamContext } from "@/server/iam-context";
import { iamOverview } from "@/server/iam-service";
import { pageError } from "@/server/page-error";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { workspaceBase, workspaceApiHref } from "@/lib/workspace";
export default async function IamPage({
  params,
}: {
  params: Promise<{ holdingSlug: string; section?: string[] }>;
}) {
  const p = await params,
    section = (p.section?.[0] ?? "overview") as IamSection;
  if (
    (p.section?.length ?? 0) > 1 ||
    !iamSections.some(([id]) => id === section)
  )
    notFound();
  const resolved = await (async () => {
    try {
      const { db, actor } = await iamContext(p.holdingSlug),
        context = await requireWorkContext(p.holdingSlug);
      return {
        data: await iamOverview(db, actor, section === "my-access"),
        scope: publicScope(context),
      };
    } catch (error) {
      return pageError(error, p.holdingSlug);
    }
  })();
  return (
    <IamConsole
      data={resolved.data}
      section={section}
      base={workspaceBase(resolved.scope)}
      api={workspaceApiHref(resolved.scope, "iam")}
    />
  );
}
