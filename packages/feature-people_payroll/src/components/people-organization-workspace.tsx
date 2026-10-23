"use client";
import type { PeopleOrganizationProps } from "@erpx/feature-people_payroll/components/people-organization-contracts";
import { OrganizationDraftWorkspace } from "@erpx/feature-people_payroll/components/organization-draft-workspace";
import { PeopleStructureWorkspace } from "@erpx/feature-people_payroll/components/people-structure-workspace";
import { emptyDraft } from "@erpx/feature-organization/lib/organization-draft";
const readOnlyDefinition = emptyDraft();

export function PeopleOrganizationWorkspace(props: PeopleOrganizationProps) {
  return props.accessOrganization && props.canManageStructure ? (
    <OrganizationDraftWorkspace
      key={`${props.actorId}:${props.currentCompanyId}`}
      base={props.base}
      companyName={
        props.data.companies.find((c) => c.id === props.currentCompanyId)?.name ?? "Company"
      }
      organization={props.accessOrganization}
      people={props}
    />
  ) : (
    <PeopleStructureWorkspace
      key={`${props.actorId}:${props.currentCompanyId}`}
      people={props}
      definition={readOnlyDefinition}
    />
  );
}
