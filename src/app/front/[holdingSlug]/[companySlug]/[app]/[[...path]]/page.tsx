import { cookies } from "next/headers";
import { notFound, redirect } from "next/navigation";
import { FrontAppShell, FrontLocationPicker, FrontPmsNavigation } from "@/components/front/front-app-shell";
import { PosConsole } from "@/components/pos/pos-console";
import { PmsWorkspaceView } from "@/components/pms/workspace";
import { WorkContextBoundary } from "@/components/workplace/work-context-boundary";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext, currentSession } from "@/server/authorization";
import { frontCompanyApp, frontCompanyBrand, frontCookieName, frontRouteForLocation, frontScopeFromLocation, type FrontAppKind } from "@/server/front-app";
import { posWorkspace } from "@/server/pos";
import { holdingResources } from "@/server/runtime";
import { one } from "@/server/pos-core";
import { pmsWorkspace } from "@/server/pms";
import { pmsCrossLinks } from "@/lib/pms-cross-links";
import { companyFeatureCatalogue } from "@/server/hospitality-rollout";
import type { Metadata } from "next";

export const dynamic = "force-dynamic";

const frontPmsSections = new Set(["dashboard", "reservations", "front-desk", "rooms", "housekeeping", "folios", "cashier", "inventory", "night-audit"]);

export async function generateMetadata({
  params,
}: {
  params: Promise<{ holdingSlug: string; companySlug: string; app: string }>;
}): Promise<Metadata> {
  const { holdingSlug, companySlug, app: rawApp } = await params;
  if (rawApp !== "pos" && rawApp !== "pms") return {};
  const app = rawApp as FrontAppKind;
  const appBase = "/front/" + encodeURIComponent(holdingSlug) + "/" + encodeURIComponent(companySlug) + "/" + app + "/";
  try {
    const entry = await frontCompanyBrand(holdingSlug, companySlug);
    if (!entry) return {};
    const title = entry.shortName + (app === "pos" ? " POS" : " PMS");
    return {
      title,
      applicationName: title,
      description: entry.companyName + (app === "pos" ? " point of sale" : " property management"),
      manifest: appBase + "manifest.webmanifest",
      icons: {
        icon: [
          { url: appBase + "icon/192", sizes: "192x192", type: "image/png" },
          { url: appBase + "icon/512", sizes: "512x512", type: "image/png" },
        ],
        apple: appBase + "apple-icon.png",
      },
    };
  } catch (error) { throw error; }
}

function locationSegmentCount(parts: string[]) {
  if (parts[0] === "holding") return 1;
  if (parts[0] === "branches" && parts[1] && parts[2] === "departments" && parts[3]) return 4;
  if ((parts[0] === "branches" || parts[0] === "departments") && parts[1]) return 2;
  return 0;
}

function queryString(values: Record<string, string | string[] | undefined>) {
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(values)) {
    if (typeof value === "string") query.set(key, value);
    else if (Array.isArray(value)) for (const item of value) query.append(key, item);
  }
  const text = query.toString();
  return text ? "?" + text : "";
}

export default async function FrontAppPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string; companySlug: string; app: string; path?: string[] }>;
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  const { holdingSlug, companySlug, app: rawApp, path = [] } = await params;
  if (rawApp !== "pos" && rawApp !== "pms") notFound();
  const app = rawApp as FrontAppKind;
  const query = await searchParams;
  let session;
  try {
    session = (await currentSession()).session;
  } catch (error) {
    if (error instanceof Error && error.message === "UNAUTHENTICATED")
      redirect("/login?next=" + encodeURIComponent("/front/" + holdingSlug + "/" + companySlug + "/" + app + "/"));
    throw error;
  }
  const entry = await frontCompanyApp(holdingSlug, companySlug);
  if (!entry) notFound();

  const cookieStore = await cookies();
  const language = cookieStore.get("erpx_front_language_" + session.user.id)?.value === "en" ? "en" : "th";
  const cookieName = frontCookieName(app, entry);
  let remembered = cookieStore.get(cookieName)?.value ?? "";
  try { remembered = decodeURIComponent(remembered); } catch { remembered = ""; }

  const locationCount = locationSegmentCount(path);
  const explicitLocation = locationCount ? path.slice(0, locationCount).join("/") : null;
  let modulePath = locationCount ? path.slice(locationCount) : [...path];
  if (app === "pos" && modulePath.length === 1 && modulePath[0] === "pos") modulePath = [];
  const explicitSection = app === "pms" ? modulePath[0] ?? "dashboard" : "";
  if (app === "pms" && (modulePath.length > 1 || !frontPmsSections.has(explicitSection))) notFound();
  if (app === "pos" && modulePath.length) notFound();

  let selectedLocation = explicitLocation;
  const forceLocationPicker = query.locations === "1";
  if (!selectedLocation && !forceLocationPicker && remembered && entry.locations.some((item) => item.path === remembered))
    selectedLocation = remembered;

  const base = "/front/" + encodeURIComponent(holdingSlug) + "/" + encodeURIComponent(companySlug);
  if (!selectedLocation || !entry.locations.some((item) => item.path === selectedLocation)) {
    const notice = remembered && !entry.locations.some((item) => item.path === remembered)
      ? (language === "th" ? "สถานที่ล่าสุดไม่มีสิทธิ์ใช้งานแล้ว กรุณาเลือกสาขาใหม่" : "Your last location is no longer available. Choose another location.")
      : undefined;
    return <FrontAppShell holdingSlug={holdingSlug} companySlug={companySlug} companyId={entry.companyId} companyName={entry.companyName} shortName={entry.shortName} accentColor={entry.accentColor} logoAttachmentId={entry.logoAttachmentId} app={app} userId={entry.userId} initialLanguage={language}>
      <FrontLocationPicker base={base} app={app} companyId={entry.companyId} userId={entry.userId} locations={entry.locations} notice={notice} />
    </FrontAppShell>;
  }

  let scope;
  try {
    scope = await frontScopeFromLocation(holdingSlug, entry.companyId, selectedLocation);
  } catch {
    notFound();
  }
  let actor;
  try {
    actor = await requireWorkContext(scope);
  } catch {
    const notice = language === "th" ? "สิทธิ์ของคุณเปลี่ยนไปแล้ว กรุณาเลือกสถานที่ที่เปิดใช้งานได้" : "Your access changed. Choose a location that is still available.";
    return <FrontAppShell holdingSlug={holdingSlug} companySlug={companySlug} companyId={entry.companyId} companyName={entry.companyName} shortName={entry.shortName} accentColor={entry.accentColor} logoAttachmentId={entry.logoAttachmentId} app={app} userId={entry.userId} initialLanguage={language}>
      <FrontLocationPicker base={base} app={app} companyId={entry.companyId} userId={entry.userId} locations={entry.locations} notice={notice} />
    </FrontAppShell>;
  }

  if (!locationCount && path.length === 0 && !forceLocationPicker) {
    const destination = frontRouteForLocation(base, app, selectedLocation) + queryString(query);
    redirect(destination);
  }

  const localPath = selectedLocation;
  const workspace = workspaceBase(publicScope(actor));
  let content;
  if (app === "pos") {
    const movement = typeof query.movement === "string"
      ? await one((await holdingResources(actor.holdingId)).write,
          "SELECT l.sale_id FROM pos_stock_moves m JOIN pos_lines l ON l.fulfillment_id=m.fulfillment_id JOIN pos_sales s ON s.id=l.sale_id WHERE m.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?) ORDER BY s.created_at LIMIT 1",
          query.movement, actor.companyId, actor.branchId, actor.branchId)
      : null;
    const data = await posWorkspace(actor, typeof query.sale === "string" ? query.sale : movement ? String(movement.sale_id) : undefined, typeof query.q === "string" ? query.q : "", typeof query.tab === "string" ? query.tab : "sale", typeof query.cursor === "string" ? query.cursor : undefined);
    content = <PosConsole initial={data} base={base} erpBase={workspace} initialTab={typeof query.tab === "string" ? query.tab : undefined} frontMode commandScopeKey={localPath} />;
  } else {
    const section = explicitSection;
    const data = await pmsWorkspace(actor, typeof query.property === "string" ? query.property : undefined, section === "night-audit");
    const enabled = process.env.NODE_ENV === "production"
      ? new Set((await companyFeatureCatalogue(actor)).filter((item) => ["Available", "Pilot", "Setup required"].includes(item.state)).map((item) => item.id))
      : null;
    content = <>
      <FrontPmsNavigation base={base} section={section} />
      <PmsWorkspaceView initial={data} base={base} erpBase={workspace} section={section} selectedFolio={typeof query.folio === "string" ? query.folio : undefined} crossLinks={pmsCrossLinks(actor, enabled ?? undefined)} frontMode />
    </>;
  }
  return <FrontAppShell holdingSlug={holdingSlug} companySlug={companySlug} companyId={entry.companyId} companyName={entry.companyName} shortName={entry.shortName} accentColor={entry.accentColor} logoAttachmentId={entry.logoAttachmentId} app={app} frontLocation={localPath} locationName={actor.scope.kind === "branch" ? actor.scope.branchName : actor.scope.kind === "department" ? actor.scope.departmentName : entry.companyName} userId={entry.userId} initialLanguage={language}>
    <WorkContextBoundary base={workspace} companyId={actor.companyId} assignmentId={actor.assignmentId} revision={actor.authority!.revision}>
      {content}
    </WorkContextBoundary>
  </FrontAppShell>;
}
