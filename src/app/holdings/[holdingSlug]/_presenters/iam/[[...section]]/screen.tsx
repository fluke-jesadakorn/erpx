import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { notFound, redirect } from "next/navigation";
import { iamSections } from "@/lib/identity_access/iam";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { peopleIamDestination } from "@/lib/people_payroll/people-lifecycle";
export default async function IamPage({
  params,
}: {
  params: Promise<{ holdingSlug: string; section?: string[] }>;
}) {
  const p = await params,
    section = p.section?.[0] ?? "overview";
  if (
    (p.section?.length ?? 0) > 1 ||
    !iamSections.some(([id]) => id === section)
  )
    notFound();
  const actor = await requireWorkContext(p.holdingSlug);
  if (section === "my-access") redirect("/my-access");
  redirect(
    canonicalUiHref(`${workspaceBase(publicScope(actor))}/${peopleIamDestination(section)}`),
  );
}
