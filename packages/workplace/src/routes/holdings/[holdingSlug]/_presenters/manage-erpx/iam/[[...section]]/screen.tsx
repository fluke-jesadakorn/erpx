import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { iamPresentationQueries } from "@erpx/server-identity_access/public/presentation-context-queries";
import { notFound } from "next/navigation";
import { IamConsole } from "@erpx/feature-iam/components/iam-console";
import { iamSections, type IamSection } from "@erpx/feature-identity_access/lib/iam";
import { requireManageErpxContext } from "@erpx/server-identity_access/public/authorization";


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
      base={canonicalUiHref("/holdings/erpx/manage-erpx")}
      api="/api/v1/platform/iam"
    />
  );
}
