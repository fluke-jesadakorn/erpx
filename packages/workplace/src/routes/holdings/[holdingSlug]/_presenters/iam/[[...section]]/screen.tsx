import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { notFound, redirect } from "next/navigation";
import { iamSections } from "@erpx/feature-identity_access/lib/iam";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { peopleIamDestination } from "@erpx/feature-people_payroll/lib/people-lifecycle";
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
