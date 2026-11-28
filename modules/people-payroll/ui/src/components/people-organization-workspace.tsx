"use client";
import type { PeopleOrganizationProps } from "@erpx/people-payroll-ui/components/people-organization-contracts";
import { OrganizationDraftWorkspace } from "@erpx/people-payroll-ui/components/organization-draft-workspace";
import { PeopleStructureWorkspace } from "@erpx/people-payroll-ui/components/people-structure-workspace";
import { emptyDraft } from "@erpx/organization-ui/lib/organization-draft";
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
