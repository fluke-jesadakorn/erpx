import { cookies, headers } from "next/headers";
import { notFound, redirect } from "next/navigation";
import { FrontAppShell, FrontLocationPicker } from "@erpx/front/components/front-app-shell";
import { WorkContextBoundary } from "@erpx/workplace/components/work-context-boundary";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext, currentSession } from "@erpx/server-identity_access/public/authorization";
import { frontCompanyApp, frontCompanyBrand, frontCookieName, frontRouteForLocation, frontScopeFromLocation, ensureFrontRole, frontPmsSections, type FrontAppKind } from "@erpx/server-work_projection/public/front-app";
import type { Metadata } from "next";
import { frontAppForHost, frontVisibleLocation, frontVisiblePath } from "@erpx/feature-shared/lib/front-host";
import { frontActionItems } from "@erpx/server-work_projection/public/front-quick-access";
import { readQuickAccessPreferences } from "@erpx/server-work_projection/public/quick-access-preferences";
import {moneyContext} from '@erpx/server-finance/public/company-money';
import Link from "@erpx/workplace/components/workspace-link";
import { consoleOrigin } from "@erpx/feature-runtime/lib/erpx-hosts";
import { appearancePreference } from "@erpx/feature-shared/lib/appearance-preference";
export const dynamic = "force-dynamic";
export async function generateMetadata({ params, }: {
    params: Promise<{
        holdingSlug: string;
        companySlug: string;
        app: string;
    }>;
}): Promise<Metadata> {
    const { holdingSlug, companySlug, app: rawApp } = await params;
    if (rawApp !== "pos" && rawApp !== "pms")
        return {};
    const app = rawApp as FrontAppKind;
    const standalone = frontAppForHost((await headers()).get("host")) === app;
    const appBase = "/front/" + encodeURIComponent(holdingSlug) + "/" + encodeURIComponent(companySlug) + "/" + app + "/";
    try {
        const entry = await frontCompanyBrand(holdingSlug, companySlug);
        if (!entry)
            return {};
        const title = entry.shortName + (app === "pos" ? " POS" : " PMS");
        return {
            title,
            applicationName: title,
            description: entry.companyName + (app === "pos" ? " point of sale" : " property management"),
            manifest: standalone ? "/manifest.webmanifest" : appBase + "manifest.webmanifest",
            icons: {
                icon: [
                    { url: standalone ? "/icon/192" : appBase + "icon/192", sizes: "192x192", type: "image/png" },
                    { url: standalone ? "/icon/512" : appBase + "icon/512", sizes: "512x512", type: "image/png" },
                ],
                apple: standalone ? "/icon/192" : appBase + "apple-icon.png",
            },
        };
    }
    catch (error) {
        throw error;
    }
}
function locationSegmentCount(parts: string[]) {
    if (parts[0] === "holding")
        return 1;
    if (parts[0] === "branches" && parts[1] && parts[2] === "departments" && parts[3])
        return 4;
    if ((parts[0] === "branches" || parts[0] === "departments") && parts[1])
        return 2;
    return 0;
}
function queryString(values: Record<string, string | string[] | undefined>) {
    const query = new URLSearchParams();
    for (const [key, value] of Object.entries(values)) {
        if (typeof value === "string")
            query.set(key, value);
        else if (Array.isArray(value))
            for (const item of value)
                query.append(key, item);
    }
    const text = query.toString();
    return text ? "?" + text : "";
}
export default async function FrontAppPage({ params, searchParams, }: {
    params: Promise<{
        holdingSlug: string;
        companySlug: string;
        app: string;
        path?: string[];
    }>;
    searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
    const { holdingSlug, companySlug, app: rawApp, path = [] } = await params;
    if (rawApp !== "pos" && rawApp !== "pms")
        notFound();
    const app = rawApp as FrontAppKind;
    const standalone = frontAppForHost((await headers()).get("host")) === app;
    const query = await searchParams;
    let session;
    try {
        session = (await currentSession()).session;
    }
    catch (error) {
        if (error instanceof Error && error.message === "UNAUTHENTICATED")
            redirect("/login?next=" + encodeURIComponent((standalone ? frontVisiblePath(holdingSlug, companySlug, path) : "/front/" + encodeURIComponent(holdingSlug) + "/" + encodeURIComponent(companySlug) + "/" + app + "/" + path.map((segment) => encodeURIComponent(segment)).join("/")) + queryString(query)));
        throw error;
    }
    const entry = await frontCompanyApp(holdingSlug, companySlug, app);
    if (!entry)
        notFound();
    const erpPickerHref = entry.locations.length ? consoleOrigin()?.toString() : undefined;
    const cookieStore = await cookies();
    const language = cookieStore.get("erpx_front_language_" + session.user.id)?.value === "en" ? "en" : app === "pos" || cookieStore.get("erpx_front_language_" + session.user.id)?.value === "th" ? "th" : "en";
    const cookieName = frontCookieName(app, entry);
    let remembered = cookieStore.get(cookieName)?.value ?? "";
    try {
        remembered = decodeURIComponent(remembered);
    }
    catch {
        remembered = "";
    }
    const locationCount = locationSegmentCount(path);
    const explicitLocation = locationCount ? path.slice(0, locationCount).join("/") : null;
    let modulePath = locationCount ? path.slice(locationCount) : [...path];
    if (app === "pos" && modulePath.length === 1 && modulePath[0] === "pos")
        modulePath = [];
    if (app === 'pos' && modulePath.length === 4 && modulePath[1] === 'receipts' && modulePath[3] === 'overview') {
        query.sale = modulePath[2];
        query.detailKind = 'pos_sale';
        query.detailId = modulePath[2];
        modulePath = [modulePath[0]];
    }
    if (app === 'pms' && modulePath.length === 5 && modulePath[1] === 'records' && ['pms_record', 'pms_entry', 'pms_hotel_sources', 'pms_hotel_record'].includes(modulePath[2]) && modulePath[4] === 'overview') {
        query.detailKind = modulePath[2];
        query.detailId = modulePath[3];
        if (modulePath[2] === 'pms_record')
            query.record = modulePath[3];
        modulePath = [modulePath[0]];
    }
    const notificationSettings = modulePath.length === 1 && modulePath[0] === "notifications";
    const explicitSection = app === "pms" ? modulePath[0] === "today" ? "dashboard" : modulePath[0] ?? "dashboard" : "";
    if (app === "pms" && !notificationSettings && (modulePath.length > 1 || !frontPmsSections.has(explicitSection)))
        notFound();
    if (app === "pos" && modulePath.length && !notificationSettings && (modulePath.length !== 1 || !["sale", "kitchen", "shifts", "history"].includes(modulePath[0])))
        notFound();
    if (app === "pos")
        query.tab = modulePath[0];
    let selectedLocation = explicitLocation;
    const forceLocationPicker = query.locations === "1";
    if (!selectedLocation && !forceLocationPicker && remembered && entry.locations.some((item) => item.path === remembered))
        selectedLocation = remembered;
    const base = "/front/" + encodeURIComponent(holdingSlug) + "/" + encodeURIComponent(companySlug);
    const frontBase = standalone ? frontVisiblePath(holdingSlug, companySlug) : base;
    if (!selectedLocation || !entry.locations.some((item) => item.path === selectedLocation)) {
        const notice = remembered && !entry.locations.some((item) => item.path === remembered)
            ? (language === "th" ? "สถานที่ล่าสุดไม่มีสิทธิ์ใช้งานแล้ว กรุณาเลือกสาขาใหม่" : "Your last location is no longer available. Choose another location.")
            : undefined;
        return <FrontAppShell holdingSlug={holdingSlug} companySlug={companySlug} companyId={entry.companyId} companyName={entry.companyName} shortName={entry.shortName} accentColor={entry.accentColor} logoAttachmentId={entry.logoPublicationId ?? entry.logoAttachmentId} app={app} standalone={standalone} erpHref={erpPickerHref} userId={entry.userId} initialLanguage={language} initialTheme={appearancePreference(cookieStore.get("erpx_appearance")?.value)}>
      <FrontLocationPicker base={frontBase} app={app} standalone={standalone} companyId={entry.companyId} userId={entry.userId} locations={entry.locations} notice={notice}/>
    </FrontAppShell>;
    }
    let scope;
    try {
        scope = await frontScopeFromLocation(holdingSlug, entry.companyId, selectedLocation);
    }
    catch {
        notFound();
    }
    const frontRoles = await ensureFrontRole(holdingSlug, entry.companyId, app, selectedLocation);
    let actor;
    try {
        actor = await requireWorkContext(scope);
    }
    catch {
        const notice = language === "th" ? "สิทธิ์ของคุณเปลี่ยนไปแล้ว กรุณาเลือกสถานที่ที่เปิดใช้งานได้" : "Your access changed. Choose a location that is still available.";
        return <FrontAppShell holdingSlug={holdingSlug} companySlug={companySlug} companyId={entry.companyId} companyName={entry.companyName} shortName={entry.shortName} accentColor={entry.accentColor} logoAttachmentId={entry.logoPublicationId ?? entry.logoAttachmentId} app={app} standalone={standalone} erpHref={erpPickerHref} userId={entry.userId} initialLanguage={language} initialTheme={appearancePreference(cookieStore.get("erpx_appearance")?.value)}>
      <FrontLocationPicker base={frontBase} app={app} standalone={standalone} companyId={entry.companyId} userId={entry.userId} locations={entry.locations} notice={notice}/>
    </FrontAppShell>;
    }
    if (!locationCount && path.length === 0 && !forceLocationPicker) {
        const destination = (standalone ? frontVisibleLocation(holdingSlug, companySlug, selectedLocation) : frontRouteForLocation(base, app, selectedLocation)) + (app === "pos" ? "sale" : "today") + queryString(query);
        redirect(destination);
    }
    const localPath = selectedLocation;
    const frontHref = standalone ? frontVisibleLocation(holdingSlug, companySlug, selectedLocation) : frontRouteForLocation(base, app, selectedLocation);
    const workspace = new URL(workspaceBase(publicScope(actor)), process.env.ERPX_PUBLIC_ORIGIN ?? "http://localhost:3100").toString();
    let content;
    {
        const sections = frontActionItems(app, actor, frontHref).map(item => item.href.slice(frontHref.length).split('?')[0]).filter(section => !section.includes('/'));
        content = <nav aria-label="Front pages" className="ui-stack">{sections.map(section => <Link className="ui-record-item" key={section} href={frontHref + section}>{section.replaceAll('-', ' ')}</Link>)}</nav>;
    }
    const quickAccessItems = frontActionItems(app, actor, frontHref);
    const quickAccessPreference = await readQuickAccessPreferences(entry.userId, "front-" + app, entry.companyId);
    const money = { ...await moneyContext(actor), canConfirmRate: false };
    return <FrontAppShell holdingSlug={holdingSlug} companySlug={companySlug} companyId={entry.companyId} companyName={entry.companyName} shortName={entry.shortName} accentColor={entry.accentColor} logoAttachmentId={entry.logoPublicationId ?? entry.logoAttachmentId} app={app} standalone={standalone} erpHref={workspace} frontLocation={localPath} locationName={actor.scope.kind === "branch" ? actor.scope.branchName : actor.scope.kind === "department" ? actor.scope.departmentName : entry.companyName} userId={entry.userId} initialLanguage={language} initialTheme={appearancePreference(cookieStore.get("erpx_appearance")?.value)} money={money} moneyApiBase={`/api/v1/front/${encodeURIComponent(holdingSlug)}/${encodeURIComponent(companySlug)}/${app}`} quickAccess={{
            items: quickAccessItems,
            preference: quickAccessPreference,
            defaultIds: quickAccessItems.slice(0, 6).map((item) => item.id),
            endpoint: "/api/v1/front/" + encodeURIComponent(holdingSlug) + "/" + encodeURIComponent(companySlug) + "/" + app + "/quick-access",
        }} workRole={{
            endpoint: "/api/v1/front/" + encodeURIComponent(holdingSlug) + "/" + encodeURIComponent(companySlug) + "/" + app + "/roles",
            assignmentId: actor.assignmentId,
            revision: actor.authority!.revision,
            options: frontRoles.options.map(option => ({ id: option.id, label: option.label ?? option.role.replaceAll("_", " ") })),
        }}>
    <WorkContextBoundary base={workspace} companyId={actor.companyId} assignmentId={actor.assignmentId} revision={actor.authority!.revision} frontApp={app} frontLocation={selectedLocation} frontRoleEndpoint={"/api/v1/front/" + encodeURIComponent(holdingSlug) + "/" + encodeURIComponent(companySlug) + "/" + app}>
      {content}
    </WorkContextBoundary>
  </FrontAppShell>;
}
