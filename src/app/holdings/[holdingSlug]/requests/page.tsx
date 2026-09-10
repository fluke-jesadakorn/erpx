import { redirect } from "next/navigation";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
export default async function Requests({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug);
  redirect(`${workspaceBase(publicScope(actor))}/work?type=service_request`);
}
