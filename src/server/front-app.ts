import "server-only";

import type { HoldingWorkspaceScope } from "@/lib/workspace";
import { holdingBindingBySlug, holdingResources } from "./runtime";
import { workspaceChoices } from "./workspace-choices";
import { currentSession } from "./authorization";
import { ApiError } from "./http";
import type { WorkspaceHoldingChoice } from "@/lib/workspace-choice";

export type FrontAppKind = "pos" | "pms";
export type FrontLocation = { key: string; label: string; path: string };

export type FrontCompanyBrand = {
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  companySlug: string;
  companyName: string;
  shortName: string;
  accentColor: string;
  logoAttachmentId: string | null;
};

export type FrontCompanyApp = FrontCompanyBrand & {
  userId: string;
  locations: FrontLocation[];
};

function localPathFromDestination(destination: string | undefined, holdingSlug: string) {
  if (!destination) return null;
  const prefix = "/holdings/" + encodeURIComponent(holdingSlug);
  if (!destination.startsWith(prefix)) return null;
  return destination.slice(prefix.length).replace(/\/home(?:\/.*)?$/, "").replace(/^\/+|\/+$/g, "") || "holding";
}

export async function frontCompanyBrand(holdingSlug: string, companySlug: string): Promise<FrontCompanyBrand | null> {
  const binding = await holdingBindingBySlug(holdingSlug);
  const { write } = await holdingResources(binding.id);
  const company = await write.prepare(
    "SELECT c.id,c.slug,c.display_name,b.short_name,b.accent_color,b.logo_attachment_id " +
    "FROM organization_nodes c LEFT JOIN storefront_app_brands b ON b.company_id=c.id " +
    "WHERE c.kind='company' AND c.slug=? AND c.status='active'",
  ).bind(companySlug).first<{
    id: string;
    slug: string;
    display_name: string;
    short_name: string | null;
    accent_color: string | null;
    logo_attachment_id: string | null;
  }>();
  if (!company) return null;

  return {
    holdingId: binding.id,
    holdingSlug: binding.slug,
    companyId: company.id,
    companySlug: company.slug,
    companyName: company.display_name,
    shortName: company.short_name?.trim() || Array.from(company.display_name.trim()).slice(0, 40).join(""),
    accentColor: company.accent_color ?? "#3659c9",
    logoAttachmentId: company.logo_attachment_id,
  };
}

export async function frontCompanyApp(holdingSlug: string, companySlug: string): Promise<FrontCompanyApp | null> {
  const brand = await frontCompanyBrand(holdingSlug, companySlug);
  if (!brand) return null;

  const holding = (await workspaceChoices()).find(
    (choice): choice is WorkspaceHoldingChoice => choice.kind === "holding" && choice.holdingSlug === brand.holdingSlug,
  );
  const authorizedCompany = holding?.companies.find((item) => item.id === brand.companyId);
  if (!authorizedCompany) return null;

  const locations: FrontLocation[] = [];
  const add = (path: string | null | undefined, label: string, key: string) => {
    if (path && !locations.some((item) => item.path === path)) locations.push({ path, label, key });
  };
  if (authorizedCompany.destination) add("holding", "Company-wide", "holding:" + brand.companyId);
  for (const branch of authorizedCompany.branches) {
    add(localPathFromDestination(branch.destination, brand.holdingSlug), branch.label, "branch:" + branch.id);
    for (const department of branch.departments)
      add(localPathFromDestination(department.destination, brand.holdingSlug), branch.label + " · " + department.label, "department:" + department.id);
  }
  for (const department of authorizedCompany.departments)
    add(localPathFromDestination(department.destination, brand.holdingSlug), department.label, "department:" + department.id);

  const { session } = await currentSession();
  return {
    ...brand,
    userId: session.user.id,
    locations,
  };
}

export async function frontScopeFromLocation(holdingSlug: string, companyId: string, location: string): Promise<HoldingWorkspaceScope> {
  const binding = await holdingBindingBySlug(holdingSlug);
  const { write } = await holdingResources(binding.id);
  if (location === "holding") return { kind: "holding", holdingSlug };
  const parts = location.split("/").filter(Boolean);
  if (parts[0] === "branches" && parts[1] && parts.length === 2) {
    const branch = await write.prepare(
      "SELECT slug FROM organization_nodes WHERE company_id=? AND kind='branch' AND slug=? AND status='active'",
    ).bind(companyId, parts[1]).first<{ slug: string }>();
    if (!branch) throw new ApiError(404, "WORKSPACE_NOT_FOUND", "This storefront location is unavailable.");
    return { kind: "branch", holdingSlug, branchSlug: branch.slug };
  }
  if (parts[0] === "departments" && parts[1] && parts.length === 2) {
    const department = await write.prepare(
      "SELECT slug FROM organization_nodes WHERE company_id=? AND kind='department' AND slug=? AND branch_id IS NULL AND status='active'",
    ).bind(companyId, parts[1]).first<{ slug: string }>();
    if (!department) throw new ApiError(404, "WORKSPACE_NOT_FOUND", "This storefront location is unavailable.");
    return { kind: "department", holdingSlug, departmentSlug: department.slug };
  }
  if (parts[0] === "branches" && parts[1] && parts[2] === "departments" && parts[3] && parts.length === 4) {
    const department = await write.prepare(
      "SELECT d.slug,b.slug AS branch_slug FROM organization_nodes d " +
      "JOIN organization_nodes b ON b.id=d.branch_id AND b.company_id=d.company_id AND b.kind='branch' AND b.status='active' " +
      "WHERE d.company_id=? AND d.kind='department' AND d.slug=? AND b.slug=? AND d.status='active'",
    ).bind(companyId, parts[3], parts[1]).first<{ slug: string; branch_slug: string }>();
    if (!department) throw new ApiError(404, "WORKSPACE_NOT_FOUND", "This storefront location is unavailable.");
    return { kind: "department", holdingSlug, branchSlug: department.branch_slug, departmentSlug: department.slug };
  }
  throw new ApiError(404, "WORKSPACE_NOT_FOUND", "Choose a valid Company location.");
}

export async function storefrontLogo(holdingSlug: string, companySlug: string) {
  const brand = await frontCompanyBrand(holdingSlug, companySlug);
  if (!brand || !brand.logoAttachmentId) return null;
  const { write, files } = await holdingResources(brand.holdingId);
  const attachment = await write.prepare(
    "SELECT a.object_key,a.media_type FROM attachments a JOIN storefront_app_brands b " +
    "ON b.company_id=? AND b.logo_attachment_id=a.id WHERE a.id=? AND a.owner_type='company_storefront_brand' AND a.revoked_at IS NULL",
  ).bind(brand.companyId, brand.logoAttachmentId).first<{ object_key: string; media_type: string }>();
  if (!attachment) return null;
  const object = await files.get(attachment.object_key);
  if (!object) return null;
  return { object, mediaType: attachment.media_type };
}

export function frontCookieName(app: FrontAppKind, entry: Pick<FrontCompanyApp, "companyId" | "userId">) {
  return ("erpx_front_" + app + "_" + entry.companyId + "_" + entry.userId).replace(/[^A-Za-z0-9_-]/g, "_");
}

export function frontLocationScope(holdingSlug: string, locationPath: string): HoldingWorkspaceScope | null {
  const clean = locationPath.replace(/^\/+|\/+$/g, "");
  if (clean === "holding") return { kind: "holding", holdingSlug };
  const parts = clean.split("/");
  if (parts[0] === "branches" && parts[1] && parts.length === 2)
    return { kind: "branch", holdingSlug, branchSlug: parts[1] };
  if (parts[0] === "departments" && parts[1] && parts.length === 2)
    return { kind: "department", holdingSlug, departmentSlug: parts[1] };
  if (parts[0] === "branches" && parts[1] && parts[2] === "departments" && parts[3] && parts.length === 4)
    return { kind: "department", holdingSlug, branchSlug: parts[1], departmentSlug: parts[3] };
  return null;
}

export function frontRouteForLocation(base: string, app: FrontAppKind, locationPath: string) {
  const location = locationPath === "holding" ? "/holding" : "/" + locationPath.replace(/^\/+/, "");
  return base + "/" + app + location + "/";
}
