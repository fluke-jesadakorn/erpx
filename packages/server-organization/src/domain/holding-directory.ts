import {BusinessRuleError} from "@erpx/server-shared/index";
export type HoldingDirectoryCompany = {
  id: string;
  slug: string;
  name: string;
};

export type HoldingDirectoryNode = {
  id: string;
  companyId: string;
  kind: "company" | "headquarters" | "branch" | "department" | "team";
  slug: string;
  name: string;
  parentId: string | null;
  branchId: string | null;
};

export type HoldingDirectoryLevel = {
  id: string;
  companyId: string;
  code: string;
  name: string;
  number: number | null;
};

export type HoldingDirectoryPosition = {
  id: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  title: string;
  levelId: string;
  reportsToPositionId: string | null;
  isCeo: boolean;
};

export type HoldingDirectoryPerson = {
  /** Stable tree/list key: one person can have multiple current engagements. */
  id: string;
  personId: string;
  userId?: string | null;
  engagementId: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  positionId: string | null;
  legalName: string;
  jobTitle: string;
  workEmail: string | null;
  workPhone: string | null;
};

export type HoldingPeopleDirectory = {
  companies: HoldingDirectoryCompany[];
  nodes: HoldingDirectoryNode[];
  levels: HoldingDirectoryLevel[];
  positions: HoldingDirectoryPosition[];
  people: HoldingDirectoryPerson[];
};


export function assertHoldingDirectoryScope(actor:{holdingId:string;companyId:string;scope:{holdingId?:string;companyId?:string}},bindingId:string){if(bindingId!==actor.holdingId||actor.scope.holdingId!==actor.holdingId||actor.scope.companyId!==actor.companyId)throw new BusinessRuleError(403,'HOLDING_SCOPE_REQUIRED','Select an authorized Holding workspace.');}
export function assertHoldingDirectoryAssignment(active:boolean){if(!active)throw new BusinessRuleError(403,'HOLDING_DIRECTORY_FORBIDDEN','Your work assignment is no longer active in this Holding.');}
