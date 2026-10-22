import type { LegacyModuleCompatibility } from "@erpx/server-identity_access/domain/legacy-module-compatibility";
export type LegacyRoutineAccess = {source:'operations/original-routines/v1';activeStaff:boolean;retainedTemplates:number;revision:string};
export type LegacyModuleWitness = {
  assignmentId: string; assignmentVersion: number; roleId: string; roleRevision: number;
  scopeKind: string; scopeNodeId: string | null; modules: string[]; validUntil: number;
  identityVersion: number; hrAppointmentVersion: number | null;
  compatibility: LegacyModuleCompatibility[];
  routineAccess?: LegacyRoutineAccess;
};
export type LegacyCompanyModuleAccess = { companyId: string; beta: boolean; observedAt: number; revision: string; witnesses: LegacyModuleWitness[] };
export type LegacyModuleAccessLease = { readonly facts: LegacyCompanyModuleAccess; assertCurrent(): Promise<void> };
