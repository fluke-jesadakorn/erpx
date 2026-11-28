import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { iamPresentationQueries } from "@erpx/identity-access-server/public/presentation-context-queries";
import { notFound } from "next/navigation";
import { IamConsole } from "@erpx/identity-access-iam-ui/components/iam-console";
import { iamSections, type IamSection } from "@erpx/identity-access-ui/lib/iam";
import { requireManageErpxContext } from "@erpx/identity-access-server/public/authorization";


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
