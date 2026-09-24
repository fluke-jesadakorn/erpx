import { notFound } from "next/navigation";
import { requireManageErpxContext } from "@/server/authorization";
import { pageError } from "@/server/page-error";
import { PlatformRoleSwitcher } from "@/components/workplace/platform-role-switcher";

export const dynamic = "force-dynamic";

export default async function ManageErpxLayout({
  children, params,
}: {
  children: React.ReactNode;
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params;
  if (holdingSlug !== "erpx") notFound();
  let context;
  try {
    context = await requireManageErpxContext();
  } catch (error) {
    return pageError(error, holdingSlug);
  }
  return <div data-platform-assignment={context.platform.assignmentId}>
    <div className="content" style={{ display: "flex", justifyContent: "flex-end", alignItems: "center", gap: 16, flexWrap: "wrap" }}>
      <PlatformRoleSwitcher activeAssignmentId={context.platform.assignmentId} assignments={context.platform.assignments} />
    </div>
    {children}
  </div>;
}
