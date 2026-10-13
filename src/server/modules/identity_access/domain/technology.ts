import type {GrantInput} from '../domain/iam-service';import type {AccessPackageDefinition} from "@/lib/people_payroll/people-lifecycle";import type {ProvisioningInput,ProvisioningPreview,AppliedPackage} from '../domain/provisioning-input';import type {DirectoryChangeInput,DirectoryInventory} from "@/lib/identity_access/directory-operations";import type {OperationEvidence} from '../domain/directory-operations';
export type PersonPlan = {
  id: string;
  personId: string;
  name: string;
  positionId: string | null;
  branchId: string | null;
  departmentId: string | null;
  engagementVersion: number;
  packageId: string;
  revision: number;
  definition: AccessPackageDefinition;
  oldIds: string[];
  users: string[];
  directoryMembers: string[];
  retained: string[];
};
export type ProvisionStep={id:string;status:string;label?:string};export type AppliedPackageFact=Omit<AppliedPackage,'scope'>&{scope:AppliedPackage['scope']|string;company_id:string};export type DirectoryMembershipFact={connection_id:string;group_id:string;member_id:string;source_id:string;application_id:string|null};export type ProvisionEngagement={id:string;person_id:string;company_id:string;position_id:string|null;branch_id:string|null;department_id:string|null;version:number;status:string;legal_name:string;user_id:string|null;person_status:string};export type ProvisionConnection={version:number;inventory:DirectoryInventory|null;write_enabled:boolean;tested_version:number|null;status:string};export type ProvisionChild={id:string;status:string;last_error:string|null};export type ProvisionGrant={id:string;assignment_id:string|null;ended_at:number|null;status:string;person_id:string;company_id:string;definition:GrantInput};export type ProvisionDirectoryPlan={change:DirectoryChangeInput;evidence:OperationEvidence;stateVersion:number};export type ProvisionEvidence={plans:PersonPlan[];directory:ProvisionDirectoryPlan[];inventories:[string,{version:number;inventory:DirectoryInventory}][];sources:DirectoryMembershipFact[];beforeApps:(string|number|null)[][]};export type BuiltProvisionPreview={result:ProvisioningPreview;evidence:ProvisionEvidence};export type ProvisionOperation={id:string;company_id:string;status:string;version:number;last_error:string|null;created_by:string;assignment_id:string;session_id:string;steps:ProvisionStep[];definition:ProvisioningInput;evidence:ProvisionEvidence};
