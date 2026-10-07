import { notFound, redirect } from "next/navigation";
import { iamSections } from "@/lib/iam";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/workspace";
import { peopleIamDestination } from "@/lib/people-lifecycle";
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
  redirect(
    `${workspaceBase(publicScope(actor))}/${peopleIamDestination(section)}`,
  );
}
