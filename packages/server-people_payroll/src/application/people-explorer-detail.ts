import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";import type {WorkContext} from '@erpx/server-identity_access/public/authorization';import type {holdingDirectorySelection} from '@erpx/server-organization/public/holding-people-directory';import type {organizationOverview} from '@erpx/server-organization/public/organization';import type {appointmentOverview} from '@erpx/server-organization/public/department-authority';import type {listBrandProfiles} from '@erpx/server-brand/public/branding';import type {administrationAccess} from '@erpx/server-runtime/public/administration';import type {staffSettings} from '@erpx/server-operations/public/staff-settings';import type {directoryOptions} from '@erpx/server-people_payroll/public/people-directory';import type {peopleLifecycleData} from '@erpx/server-people_payroll/public/people-lifecycle';import type {getPerson,payrollOptions} from '@erpx/server-people_payroll/public/people-payroll';import {BusinessRuleError} from '@erpx/server-shared/index';import {peopleExplorerTabs,type OrganizationSelection,type PeopleExplorerChangeSearch} from '@erpx/server-people_payroll/domain/people-explorer-detail';
export interface PeopleExplorerPorts{prepare(actor:WorkContext):Promise<void>;selection(actor:WorkContext,selection:OrganizationSelection,archived:boolean):ReturnType<typeof holdingDirectorySelection>;organization(actor:WorkContext,search:Parameters<typeof organizationOverview>[2]):ReturnType<typeof organizationOverview>;person(actor:WorkContext,id:string,engagementId:string):ReturnType<typeof getPerson>;directoryOptions(actor:WorkContext):ReturnType<typeof directoryOptions>;staff(actor:WorkContext):ReturnType<typeof staffSettings>;payroll(actor:WorkContext):ReturnType<typeof payrollOptions>;lifecycle(actor:WorkContext):ReturnType<typeof peopleLifecycleData>;branding(actor:WorkContext):ReturnType<typeof listBrandProfiles>;administration(actor:WorkContext):ReturnType<typeof administrationAccess>;authority(actor:WorkContext):ReturnType<typeof appointmentOverview>;history(engagementId:unknown):Promise<Record<string,unknown>[]>;}
export function createPeopleExplorerApplication(ports:PeopleExplorerPorts){async function peopleExplorerDetail(actor: WorkContext, selection: OrganizationSelection, tab: string, archived = false, changeSearch: PeopleExplorerChangeSearch = {}) {
  const canManage = hasPermission(actor, "people.manage");
  const target = await ports.selection(actor, selection, archived);
  const person = target?.person;
  const companyId = target?.companyId;
  if (!companyId) throw new BusinessRuleError(404, "EXPLORER_ITEM_NOT_FOUND", "This organization item is no longer available.");
  const local = companyId === actor.companyId;
  const canReadPay = hasPermission(actor, "people.compensation.read") || hasPermission(actor, "people.compensation.payroll");
  const tabs = peopleExplorerTabs(selection.kind, local && canManage, local && canReadPay);
  if (!tabs.some(t => t.toLowerCase() === tab.toLowerCase())) throw new BusinessRuleError(403, "EXPLORER_TAB_FORBIDDEN", "This detail section is not available to your role.");
  const search = { ...(["changes","settings"].includes(tab.toLowerCase()) ? changeSearch : {}), person: person?.id, department: selection.kind === "department" ? selection.id : undefined, team: selection.kind === "team" ? selection.id : undefined, tab: tab.toLowerCase() };
  const base = canonicalUiHref(`/holdings/${actor.holdingSlug}`);
  const selected = person;
  const currentCompany = selected?.companyId === actor.companyId;
  const restrictedDetail = !!selected && currentCompany && (canManage || canReadPay);
  await ports.prepare(actor);
  const [organization, privatePerson, editorNodes, staffOptions, groups, lifecycle, branding, authority] = await Promise.all([
    local && (canManage || canReadPay && selection.kind === "position" && search.tab === "compensation") ? ports.organization(actor,search) : Promise.resolve(null),
    restrictedDetail ? ports.person(actor, selected!.personId, selected!.engagementId).then(detail => canManage ? detail : {
      ...detail,
      person: {
        id: detail.person.id,
        engagement_id: detail.person.engagement_id,
        engagement_version: detail.person.engagement_version,
        pay_group_id: detail.person.pay_group_id,
        currency: detail.person.currency,
      },
      leave: [], time: [], recommendedPay: null, recommendedLeave: null, leaveAssignment: null,
    }).catch(() => null) : Promise.resolve(null),
    restrictedDetail && canManage && search.tab === "profile" ? ports.directoryOptions(actor) : Promise.resolve([]),
    restrictedDetail && canManage && search.tab === "profile" ? ports.staff(actor) : Promise.resolve(null),
    restrictedDetail && search.tab === "compensation" ? ports.payroll(actor).catch(() => []) : Promise.resolve([]),
    canManage && local && (restrictedDetail && ["employment","history"].includes(search.tab) || search.tab === "settings") ? ports.lifecycle(actor) : Promise.resolve(null),
    canManage && local && search.tab === "settings" ? Promise.all([ports.branding(actor), ports.administration(actor)]) : Promise.resolve(null),
    canManage && local && (search.department || search.team) ? ports.authority(actor) : Promise.resolve(null),
  ]);
  const history = privatePerson && search.tab === "history" ? await ports.history(privatePerson.person.engagement_id ?? null) : [];
  const brandingProps = branding && organization ? {
    base,
    ...branding[0],
    canHolding: branding[1].owner,
    canBranches: branding[1].manageOrganization,
    canStorefront: branding[1].manageOrganization && actor.scope.kind === "holding",
    canReviewPublications: hasPermission(actor, "records.review") || hasPermission(actor, "commerce.approve"),
  } : null;
  // Pay and private contact fields are sent only for their authorized section.
  if (privatePerson && search.tab !== "compensation") {
    for (const key of ["base_amount_minor", "hourly_rate_minor", "compensation_effective_from", "pay_frequency"]) delete privatePerson.person[key];
    privatePerson.revisions = []; privatePerson.recommendedPay = null;
  }
  return { selection, tab: tabs.find(t => t.toLowerCase() === tab.toLowerCase())!, tabs, privatePerson, organization, groups, editorNodes, staffOptions, lifecycle, history, branding: brandingProps, authority: authority };
}
return {peopleExplorerDetail};}
