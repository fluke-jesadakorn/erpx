import type { ComponentProps } from "react";
import type { WorkContext } from "./authorization";
import type { OrganizationSelection } from "@/components/people/organization-explorer";
import { ApiError } from "./http";
import { peopleExplorerTabs } from "@/lib/people-explorer";
import type { DepartmentAuthorityWorkspace } from "@/components/modules/department-authority-workspace";
import { hasPermission } from "@/lib/permissions";
import { administrationAccess } from "@/server/administration";
import { listBrandProfiles } from "@/server/branding";
import { appointmentOverview } from "@/server/department-authority";
import { holdingDirectorySelection } from "@/server/holding-people-directory";
import { organizationOverview } from "@/server/organization";
import { directoryOptions } from "@/server/people-directory";
import { peopleLifecycleData } from "@/server/people-lifecycle";
import { getPerson, payrollOptions } from "@/server/people-payroll";
import { holdingResources } from "@/server/runtime";
import { staffSettings } from "@/server/staff-settings";

export type PeopleExplorerChangeSearch = Partial<Record<"changeId" | "changeState" | "changeQuery" | "changePage", string>>;

export async function peopleExplorerDetail(actor: WorkContext, selection: OrganizationSelection, tab: string, archived = false, changeSearch: PeopleExplorerChangeSearch = {}) {
  const canManage = hasPermission(actor, "people.manage");
  const target = await holdingDirectorySelection(actor, selection, archived);
  const person = target?.person;
  const companyId = target?.companyId;
  if (!companyId) throw new ApiError(404, "EXPLORER_ITEM_NOT_FOUND", "This organization item is no longer available.");
  const local = companyId === actor.companyId;
  const canReadPay = hasPermission(actor, "people.compensation.read") || hasPermission(actor, "people.compensation.payroll");
  const tabs = peopleExplorerTabs(selection.kind, local && canManage, local && canReadPay);
  if (!tabs.some(t => t.toLowerCase() === tab.toLowerCase())) throw new ApiError(403, "EXPLORER_TAB_FORBIDDEN", "This detail section is not available to your role.");
  const search = { ...(tab.toLowerCase() === "changes" ? changeSearch : {}), person: person?.id, department: selection.kind === "department" ? selection.id : undefined, team: selection.kind === "team" ? selection.id : undefined, tab: tab.toLowerCase() };
  const base = `/holdings/${actor.holdingSlug}`;
  const selected = person;
  const currentCompany = selected?.companyId === actor.companyId;
  const restrictedDetail = !!selected && currentCompany && (canManage || canReadPay);
  const { write } = await holdingResources(actor.holdingId);
  const [organization, privatePerson, editorNodes, staffOptions, groups, lifecycle, branding, authority] = await Promise.all([
    local && (canManage || canReadPay && selection.kind === "position" && search.tab === "compensation") ? organizationOverview(write, actor, search) : Promise.resolve(null),
    restrictedDetail ? getPerson(actor, selected!.personId, selected!.engagementId).then(detail => canManage ? detail : {
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
    restrictedDetail && canManage && search.tab === "profile" ? directoryOptions(actor) : Promise.resolve([]),
    restrictedDetail && canManage && search.tab === "profile" ? staffSettings(write, actor) : Promise.resolve(null),
    restrictedDetail && search.tab === "compensation" ? payrollOptions(actor).catch(() => []) : Promise.resolve([]),
    canManage && local && (restrictedDetail && ["employment","history"].includes(search.tab) || search.tab === "settings") ? peopleLifecycleData(write, actor) : Promise.resolve(null),
    canManage && local && search.tab === "settings" ? Promise.all([listBrandProfiles(write, actor), administrationAccess(actor)]) : Promise.resolve(null),
    canManage && local && (search.department || search.team) ? appointmentOverview(write, actor) : Promise.resolve(null),
  ]);
  const history = privatePerson && search.tab === "history" ? (await write.prepare(
    "SELECT h.created_at,h.reason,l.name AS level_name,old.name AS previous_level_name FROM staff_level_history h JOIN staff_levels l ON l.id=h.level_id LEFT JOIN staff_levels old ON old.id=h.previous_level_id WHERE h.engagement_id=? ORDER BY h.created_at DESC",
  ).bind(privatePerson.person.engagement_id ?? null).all()).results : [];
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
  return { selection, tab: tabs.find(t => t.toLowerCase() === tab.toLowerCase())!, tabs, privatePerson, organization, groups, editorNodes, staffOptions, lifecycle, history, branding: brandingProps, authority: authority as ComponentProps<typeof DepartmentAuthorityWorkspace>["initial"] | null };
}
