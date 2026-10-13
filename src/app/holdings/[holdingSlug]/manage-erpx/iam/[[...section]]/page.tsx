import { iamPresentationQueries } from "@/server/modules/identity_access/public/presentation-context-queries";
import { notFound } from "next/navigation";
import { IamConsole } from "@/components/iam/iam-console";
import { iamSections, type IamSection } from "@/lib/identity_access/iam";
import { requireManageErpxContext } from "@/server/modules/identity_access/public/authorization";


export default async function IamPage({
  params,
}: {
  params: Promise<{ section?: string[] }>;
}) {
  const p = await params,
    section = (p.section?.[0] ?? "overview") as IamSection;
  if (
    (p.section?.length ?? 0) > 1 ||
    !iamSections.some(([id]) => id === section)
  )
    notFound();
  await requireManageErpxContext();
  const { queries, actor } = await iamPresentationQueries(null);
  return (
    <IamConsole
      data={await queries.iamOverview(actor, section === "my-access")}
      section={section}
      base="/holdings/erpx/manage-erpx"
      api="/api/v1/platform/iam"
    />
  );
}
