import { notFound } from "next/navigation";
import { IamConsole } from "@/components/iam/iam-console";
import { iamSections, type IamSection } from "@/lib/iam";
import { iamContext } from "@/server/iam-context";
import { iamOverview } from "@/server/iam-service";
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
  const { db, actor } = await iamContext(null);
  return (
    <IamConsole
      data={await iamOverview(db, actor, section === "my-access")}
      section={section}
      base="/platform"
      api="/api/v1/platform/iam"
    />
  );
}
