import { cookies, headers } from "next/headers";
import { redirect } from "next/navigation";
import { frontAppForHost, frontVisiblePath } from "@/lib/shared/front-host";
import { currentSession } from "@/server/modules/identity_access/public/authorization";
import { frontCompanyApp } from "@/server/modules/work_projection/public/front-app";
import { frontCompanyChoices, frontCompanyItems } from "@/server/modules/work_projection/public/front-quick-access";
import { readQuickAccessPreferences } from "@/server/modules/work_projection/public/quick-access-preferences";
import { StandardFrontCompanyPicker } from "@/components/front/standard-front-access";
import { appsEntryUrl } from "@/lib/runtime/application-origins";
import { workspaceChoices } from "@/server/modules/organization/public/workspace-choices";

export const dynamic = "force-dynamic";

export default async function FrontStart({ searchParams }: { searchParams: Promise<{ choose?: string }> }) {
  const app = frontAppForHost((await headers()).get("host"));
  if (!app) redirect("/login");
  let session;
  try { session = (await currentSession()).session; } catch (error) {
    if (error instanceof Error && error.message === "UNAUTHENTICATED") redirect("/login");
    throw error;
  }
  const chooseCompany = (await searchParams).choose === "1";
  const remembered = (await cookies()).get("erpx_front_device_target")?.value;
  if (remembered && !chooseCompany) {
    const parts = remembered.split("/").filter(Boolean);
    if (parts.length >= 3) {
      const entry = await frontCompanyApp(parts[0], parts[1], app).catch(() => null);
      if (entry && entry.locations.some((location) => remembered.includes("/" + location.path + "/"))) redirect(remembered);
    }
  }
  const companies = await frontCompanyChoices(app);
  if (companies.length === 1) redirect(frontVisiblePath(companies[0].holding, companies[0].slug));
  const items = frontCompanyItems(companies);
  const preference = await readQuickAccessPreferences(session.user.id, "front-picker", app);
  const erpHref = (await workspaceChoices()).length ? appsEntryUrl() : null;
  return <StandardFrontCompanyPicker app={app} companies={companies} items={items} preference={preference} erpHref={erpHref} />;
}
