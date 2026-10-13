import type {HoldingWorkspaceScope} from "@/lib/work_projection/workspace";
import type {WorkspaceHoldingChoice} from "@/lib/work_projection/workspace-choice";
import type {WorkContext,AccountAccessContext} from "../../identity_access/public/authorization";
import {assignmentApplies} from "@/server/modules/identity_access/public/rules/workspace-access";
import {frontVisibleLocation} from "../domain/front-path";
import {frontRouteForLocation,localPathFromDestination,type FrontAppKind,type FrontCompanyApp,type FrontCompanyBrand,type FrontLocation} from "../domain/front-app";
import type {readWorkSelection,selectWorkRole} from "../public/work-role-store";
import type {publishedBrandIdentity} from "../../brand/public/brand-identity";
import type {StoredFile} from "../../runtime/public/runtime/types";
export type FrontFacts={companyBrand(companySlug:string):Promise<{
    id: string;
    slug: string;
    display_name: string;
    short_name: string | null;
    accent_color: string | null;
    logo_attachment_id: string | null;
    logo_publication_id:string|null;
  }|null>;
branch(companyId:string,value:string):Promise<{ slug: string }|null>;
department(companyId:string,value:string):Promise<{ slug: string }|null>;
branchDepartment(companyId:string,value:string,value2:string):Promise<{ slug: string; branch_slug: string }|null>;
logoPublication(companyId:string,companyId2:string,logoPublicationId:string):Promise<{object_key:string;media_type:string}|null>;
actorCompany(companyId:string,companyId2:string):Promise<{ slug: string }|null>;};
export type FrontPorts={facts:FrontFacts;error(status:number,code:string,message:string):Error;isApiError(error:unknown):error is Error&{code:string};now():number;prepareSlug(slug:string):Promise<{id:string;slug:string}>;prepareHolding(id:string):Promise<void>;accountAccess(scope:HoldingWorkspaceScope):Promise<AccountAccessContext>;roles(companyId:string,ids:string[]):Promise<Map<string,{permissions:string[]}>>;selection(sessionId:string,userId:string,companyId:string):ReturnType<typeof readWorkSelection>;select(input:Parameters<typeof selectWorkRole>[1]):ReturnType<typeof selectWorkRole>;publishedBrand(companyId:string):ReturnType<typeof publishedBrandIdentity>;workspaces():Promise<(WorkspaceHoldingChoice|{kind:"platform"})[]>;currentUserId():Promise<string>;file(key:string):Promise<StoredFile|null>;origin(app:FrontAppKind):URL|null;};
export function createFrontAppService(ports:FrontPorts){
async function frontRoleState(holdingSlug: string, companyId: string, app: FrontAppKind, location: string) {
  const scope = await frontScopeFromLocation(holdingSlug, companyId, location);
  const access = await ports.accountAccess(scope);
  if (access.scope.companyId !== companyId) throw ports.error(404, "FRONT_NOT_FOUND", "This Front app is unavailable.");
  await ports.prepareHolding(access.scope.holdingId);
  const definitions = await ports.roles(companyId, access.assignments.map((item) => item.role));
  const options = access.scope.kind === "department" ? [] : access.assignments.filter((assignment) =>
    assignmentApplies(assignment, access.scope) && definitions.get(assignment.role)?.permissions.includes(app + ".read"),
  );
  const selection = await ports.selection( access.sessionId, access.userId, companyId);
  const selected = selection && selection.expires_at > ports.now()
    ? options.find((item) => item.id === selection.assignment_id && item.version === selection.assignment_version)
    : undefined;
  return { access, options, selection, selected };
}
async function ensureFrontRole(holdingSlug: string, companyId: string, app: FrontAppKind, location: string) {
  let state = await frontRoleState(holdingSlug, companyId, app, location);
  if (state.selected) return state;
  const assignment = state.options[0];
  if (!assignment) throw ports.error(403, "FRONT_ACCESS_DENIED", "No active assignment can open this Front workspace.");
  try {
    await ports.select( {
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
    if (!(ports.isApiError(error)) || error.code !== "WORK_CONTEXT_CHANGED") throw error;
  }
  state = await frontRoleState(holdingSlug, companyId, app, location);
  if (!state.selected) throw ports.error(409, "WORK_CONTEXT_CHANGED", "Your work access changed. Reload this page.");
  return state;
}
async function frontCompanyBrand(holdingSlug: string, companySlug: string): Promise<FrontCompanyBrand | null> {
  const binding = await ports.prepareSlug(holdingSlug);
  await ports.prepareHolding(binding.id);
  const company = await ports.facts.companyBrand(companySlug);
  if (!company) return null;
  const published=await ports.publishedBrand(company.id);

  return {
    holdingId: binding.id,
    holdingSlug: binding.slug,
    companyId: company.id,
    companySlug: company.slug,
    companyName: company.display_name,
    shortName: published?published.identity.identity.shortName:company.short_name?.trim() || Array.from(company.display_name.trim()).slice(0, 40).join(""),
    accentColor: published?.identity.visual.accentColor??company.accent_color ?? "#3659c9",
    logoAttachmentId: published?null:company.logo_attachment_id,
    logoPublicationId:published?published.identity.visual.logoPublicationId:company.logo_publication_id,
  };
}
async function frontCompanyApp(holdingSlug: string, companySlug: string, app?: FrontAppKind): Promise<FrontCompanyApp | null> {
  const brand = await frontCompanyBrand(holdingSlug, companySlug);
  if (!brand) return null;

  const holding = (await ports.workspaces()).find(
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

  const userId = await ports.currentUserId();
  return {
    ...brand,
    userId,
    locations,
  };
}
async function frontScopeFromLocation(holdingSlug: string, companyId: string, location: string): Promise<HoldingWorkspaceScope> {
  const binding = await ports.prepareSlug(holdingSlug);
  await ports.prepareHolding(binding.id);
  if (location === "holding") return { kind: "holding", holdingSlug };
  const parts = location.split("/").filter(Boolean);
  if (parts[0] === "branches" && parts[1] && parts.length === 2) {
    const branch = await ports.facts.branch(companyId,parts[1]);
    if (!branch) throw ports.error(404, "WORKSPACE_NOT_FOUND", "This storefront location is unavailable.");
    return { kind: "branch", holdingSlug, branchSlug: branch.slug };
  }
  if (parts[0] === "departments" && parts[1] && parts.length === 2) {
    const department = await ports.facts.department(companyId,parts[1]);
    if (!department) throw ports.error(404, "WORKSPACE_NOT_FOUND", "This storefront location is unavailable.");
    return { kind: "department", holdingSlug, departmentSlug: department.slug };
  }
  if (parts[0] === "branches" && parts[1] && parts[2] === "departments" && parts[3] && parts.length === 4) {
    const department = await ports.facts.branchDepartment(companyId,parts[3],parts[1]);
    if (!department) throw ports.error(404, "WORKSPACE_NOT_FOUND", "This storefront location is unavailable.");
    return { kind: "department", holdingSlug, branchSlug: department.branch_slug, departmentSlug: department.slug };
  }
  throw ports.error(404, "WORKSPACE_NOT_FOUND", "Choose a valid Company location.");
}
async function storefrontLogo(holdingSlug: string, companySlug: string) {
  const brand = await frontCompanyBrand(holdingSlug, companySlug);
  if (!brand?.logoPublicationId) return null;
  await ports.prepareHolding(brand.holdingId);
  const publication=await ports.facts.logoPublication(brand.companyId,brand.companyId,brand.logoPublicationId);
  if(!publication)return null;
  const object = await ports.file(publication.object_key);
  if (!object) return null;
  return { object, mediaType: publication.media_type };
}
async function frontWorkspaceHref(actor: WorkContext, app: FrontAppKind) {
  await ports.prepareHolding(actor.holdingId);
  const company = await ports.facts.actorCompany(actor.companyId,actor.companyId);
  if (!company) return null;

  const location = actor.scope.kind === "holding"
    ? "holding"
    : actor.scope.kind === "branch"
      ? "branches/" + encodeURIComponent(actor.scope.branchSlug)
      : actor.scope.branchSlug
        ? "branches/" + encodeURIComponent(actor.scope.branchSlug) + "/departments/" + encodeURIComponent(actor.scope.departmentSlug)
        : "departments/" + encodeURIComponent(actor.scope.departmentSlug);
  const origin=ports.origin(app);
  if(origin)return new URL(frontVisibleLocation(actor.holdingSlug,company.slug,location),origin).toString();
  return frontRouteForLocation(
    "/front/" + encodeURIComponent(actor.holdingSlug) + "/" + encodeURIComponent(company.slug),
    app,
    location,
  );
}
return {frontRoleState,ensureFrontRole,frontCompanyBrand,frontCompanyApp,frontScopeFromLocation,storefrontLogo,frontWorkspaceHref};
}
