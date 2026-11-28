import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { notFound, redirect } from "next/navigation";
import { iamSections } from "@erpx/identity-access-ui/lib/iam";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { peopleIamDestination } from "@erpx/people-payroll-ui/lib/people-lifecycle";
export default async function IamPage({
  params, searchParams,
}: {
  params: Promise<{ holdingSlug: string; section?: string[] }>;
  searchParams?: Promise<Record<string,string|string[]|undefined>>;
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
  const query=new URLSearchParams();for(const [key,value] of Object.entries(await searchParams??{}))for(const item of Array.isArray(value)?value:value?[value]:[])query.append(key,item);
  const destination=peopleIamDestination(section);
  redirect(
    canonicalUiHref(`${workspaceBase(publicScope(actor))}/${destination}${destination.includes("?")?"&":"?"}${query}`),
  );
}
