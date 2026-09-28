import "server-only";

import type { HoldingWorkspaceScope } from "@/lib/workspace";
import { holdingBindingBySlug, holdingResources } from "./runtime";
import { workspaceChoices } from "./workspace-choices";
import { currentSession, requireAccountAccess } from "./authorization";
import type { WorkContext } from "./authorization";
import { ApiError } from "./http";
import type { WorkspaceHoldingChoice } from "@/lib/workspace-choice";
import { frontOrigin, frontVisibleLocation } from "@/lib/front-host";
import { assignmentApplies } from "@/lib/workspace-access";
import { iamRoles } from "./iam-store";
import { readWorkSelection, selectWorkRole } from "./work-role-store";

export type FrontAppKind = "pos" | "pms";
export type FrontLocation = { key: string; label: string; path: string };

/**
 * Commands that belong to a staff member's live work surface. Configuration,
 * reconciliation, reporting and independent decisions intentionally stay on
 * the ERPX routes even when the same account also has administrator access.
 */
const frontPosOperations = new Set([
  "sale.create", "sale.add", "sale.remove", "sale.hold", "sale.resume",
  "sale.cancel", "sale.complete", "payment.create", "payment.retry",
  "payment.cancel", "payment.cancel_transfer", "refund.request",
  "kitchen.send", "kitchen.start", "kitchen.ready", "kitchen.deliver",
  "kitchen.cancel", "shift.open", "shift.close", "cash.create",
]);

const frontPmsOperations = new Set([
  "reservation.create", "reservation.cancel", "reservation.no_show",
  "stay.cancel", "stay.no_show", "stay.assign", "stay.move", "stay.guest",
  "stay.route_folio", "stay.check_in", "stay.preview_checkout",
  "stay.check_out", "stay.change_dates", "charge.add", "payment.receive",
  "payment.deposit", "payment.apply_deposit", "payment.request_transfer",
  "payment.cancel_transfer", "refund.request", "folio.close", "shift.open",
  "cash.move", "shift.close", "task.create", "task.assign", "task.start",
  "task.complete", "task.inspect", "room.dnd", "room.observe",
  "maintenance.open", "maintenance.close",
  "night.start", "night.resume",
]);

export function frontOperationAllowed(app: FrontAppKind, operation: string) {
  return (app === "pos" ? frontPosOperations : frontPmsOperations).has(operation);
}

export const frontPmsSections = new Set([
  "dashboard", "reservations", "front-desk", "rooms", "housekeeping",
  "folios", "cashier", "night-audit",
]);

export type FrontCompanyBrand = {
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  companySlug: string;
  companyName: string;
  shortName: string;
  accentColor: string;
  logoAttachmentId: string | null;
  logoPublicationId: string | null;
};

export type FrontCompanyApp = FrontCompanyBrand & {
  userId: string;
  locations: FrontLocation[];
};

export async function frontRoleState(holdingSlug: string, companyId: string, app: FrontAppKind, location: string) {
  const scope = await frontScopeFromLocation(holdingSlug, companyId, location);
  const access = await requireAccountAccess(scope);
  if (access.scope.companyId !== companyId) throw new ApiError(404, "FRONT_NOT_FOUND", "This Front app is unavailable.");
  const { write } = await holdingResources(access.scope.holdingId);
  const definitions = await iamRoles(write, companyId, access.assignments.map((item) => item.role));
  const options = access.scope.kind === "department" ? [] : access.assignments.filter((assignment) =>
    assignmentApplies(assignment, access.scope) && definitions.get(assignment.role)?.permissions.includes(app + ".read"),
  );
  const selection = await readWorkSelection(write, access.sessionId, access.userId, companyId);
  const selected = selection && selection.expires_at > Date.now()
    ? options.find((item) => item.id === selection.assignment_id && item.version === selection.assignment_version)
    : undefined;
  return { access, write, options, selection, selected };
}

/** Open a Front workspace with an existing eligible assignment, without a role picker. */
export async function ensureFrontRole(holdingSlug: string, companyId: string, app: FrontAppKind, location: string) {
  let state = await frontRoleState(holdingSlug, companyId, app, location);
  if (state.selected) return state;
  const assignment = state.options[0];
  if (!assignment) throw new ApiError(403, "FRONT_ACCESS_DENIED", "No active assignment can open this Front workspace.");
  try {
    await selectWorkRole(state.write, {
      sessionId: state.access.sessionId,
      userId: state.access.userId,
      companyId,
      assignmentId: assignment.id,
      assignmentVersion: assignment.version!,
      revision: state.selection?.revision ?? 0,
      expiresAt: state.access.expiresAt,
      rememberPreference: false,
    });
  } catch (error) {
    // A concurrent tab may have selected a role; use it if it still permits this app.
    if (!(error instanceof ApiError) || error.code !== "WORK_CONTEXT_CHANGED") throw error;
  }
  state = await frontRoleState(holdingSlug, companyId, app, location);
  if (!state.selected) throw new ApiError(409, "WORK_CONTEXT_CHANGED", "Your work access changed. Reload this page.");
  return state;
}

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
    "SELECT c.id,c.slug,c.display_name,b.short_name,b.accent_color,b.logo_attachment_id,b.logo_publication_id " +
    "FROM organization_nodes c LEFT JOIN storefront_app_brands b ON b.company_id=c.id " +
    "WHERE c.kind='company' AND c.slug=? AND c.status='active'",
  ).bind(companySlug).first<{
    id: string;
    slug: string;
    display_name: string;
    short_name: string | null;
    accent_color: string | null;
    logo_attachment_id: string | null;
    logo_publication_id:string|null;
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
    logoPublicationId:company.logo_publication_id,
  };
}

export async function frontCompanyApp(holdingSlug: string, companySlug: string, app?: FrontAppKind): Promise<FrontCompanyApp | null> {
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

  if (app) {
    for (let index = locations.length - 1; index >= 0; index--) {
      try {
        const roles = await frontRoleState(brand.holdingSlug, brand.companyId, app, locations[index].path);
        if (!roles.options.length) locations.splice(index, 1);
      } catch { locations.splice(index, 1); }
    }
  }

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
  if (!brand?.logoPublicationId) return null;
  const { write, files } = await holdingResources(brand.holdingId);
  const publication=await write.prepare(`SELECT p.derivative_object_key object_key,p.derivative_media_type media_type FROM attachment_publications p
    JOIN storefront_app_brands b ON b.company_id=? AND b.logo_publication_id=p.id WHERE p.id=? AND p.state='approved'`)
    .bind(brand.companyId,brand.logoPublicationId).first<{object_key:string;media_type:string}>();
  if(!publication)return null;
  const object = await files.get(publication.object_key);
  if (!object) return null;
  return { object, mediaType: publication.media_type };
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

/**
 * Point operational work back to the employee Front for the actor's current
 * company/location. The ERPX work queue still owns approvals and reviews.
 */
export async function frontWorkspaceHref(actor: WorkContext, app: FrontAppKind) {
  const { write } = await holdingResources(actor.holdingId);
  const company = await write.prepare(
    "SELECT slug FROM organization_nodes WHERE id=? AND company_id=? AND kind='company' AND status='active'",
  ).bind(actor.companyId, actor.companyId).first<{ slug: string }>();
  if (!company) return null;

  const location = actor.scope.kind === "holding"
    ? "holding"
    : actor.scope.kind === "branch"
      ? "branches/" + encodeURIComponent(actor.scope.branchSlug)
      : actor.scope.branchSlug
        ? "branches/" + encodeURIComponent(actor.scope.branchSlug) + "/departments/" + encodeURIComponent(actor.scope.departmentSlug)
        : "departments/" + encodeURIComponent(actor.scope.departmentSlug);
  const origin=frontOrigin(app);
  if(origin)return new URL(frontVisibleLocation(actor.holdingSlug,company.slug,location),origin).toString();
  return frontRouteForLocation(
    "/front/" + encodeURIComponent(actor.holdingSlug) + "/" + encodeURIComponent(company.slug),
    app,
    location,
  );
}
