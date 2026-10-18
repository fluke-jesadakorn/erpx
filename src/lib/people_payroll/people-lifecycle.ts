import { z } from "zod";
import { policyGrantInput } from "@/lib/identity_access/user-management";

const id = z.string().min(1).max(180);
export const packageDefinition = z
  .object({
    name: z.string().trim().min(2).max(160),
    account: z.enum(["local", "univention", "active_directory"]),
    connectionId: id.nullable().default(null),
    grants: z.array(policyGrantInput).max(20),
    groupIds: z.array(id).max(50),
    directoryGroupIds: z.array(id).max(50).default([]),
    windowsPolicies: z
      .array(
        z
          .object({
            id,
            name: z.string().min(1).max(160),
            processor: z.enum(["admx", "preferences"]),
            targetOu: id,
            securityGroup: id.nullable().default(null),
            computerTarget: id.nullable().default(null),
            scope: z.enum(["user", "computer"]),
          })
          .refine(
            (v) => v.scope !== "computer" || !!v.computerTarget,
            "Computer policies need an explicit computer group or OU.",
          ),
      )
      .max(30)
      .default([]),
    tasks: z
      .array(
        z.object({
          kind: z.enum([
            "equipment",
            "drive",
            "printer",
            "directory",
            "handover",
          ]),
          label: z.string().trim().min(2).max(300),
        }),
      )
      .max(50),
  })
  .superRefine((v, ctx) => {
    if (v.account !== "local" && !v.connectionId)
      ctx.addIssue({
        code: "custom",
        message: "Choose a directory connection.",
      });
    if (
      (v.directoryGroupIds.length || v.windowsPolicies.length) &&
      !v.connectionId
    )
      ctx.addIssue({
        code: "custom",
        message: "Directory groups and Windows policies require a connection.",
      });
    if (
      new Set(v.groupIds).size !== v.groupIds.length ||
      new Set(v.directoryGroupIds).size !== v.directoryGroupIds.length
    )
      ctx.addIssue({ code: "custom", message: "Choose each group once." });
  });
export type AccessPackageDefinition = z.infer<typeof packageDefinition>;
export type AccessPackage = {
  id: string;
  version: number;
  published_revision: number;
  definition: AccessPackageDefinition;
};
export const packageInput = z.object({
  id: id.optional(),
  expectedVersion: z.number().int().nonnegative(),
  definition: packageDefinition,
  reason: z.string().trim().min(3).max(1000),
});
export const lifecycleInput = z.object({
  id: id.optional(),
  expectedVersion: z.number().int().nonnegative(),
  kind: z.enum(["onboarding", "transfer", "offboarding", "suspend"]),
  personId: id,
  engagementId: id,
  positionId: id.nullable(),
  managerPersonId: id.nullable().default(null),
  acceptRecommendedPay: z.boolean().default(false),
  acceptRecommendedLeave: z.boolean().default(false),
  effectiveAt: z.number().int().positive(),
  departureReason: z.enum(["retirement","other"]).default("other"),
  timezone: z.string().min(1).max(80).refine(value => {try {new Intl.DateTimeFormat("en",{timeZone:value});return true;} catch{return false;}}, "Choose a valid timezone.").default("Asia/Bangkok"),
  clearanceOwners: z.object({equipment:id.nullable().default(null),payroll:id.nullable().default(null),finance:id.nullable().default(null)}).default({equipment:null,payroll:null,finance:null}),
  lastWorkingDate: z.iso.date().nullable().default(null),
  reason: z.string().trim().min(3).max(1000),
});
export type LifecycleInput = z.infer<typeof lifecycleInput>;
export type LifecycleCase = {
  workflow?: "legacy" | "placement";
  cutoffEffective?: boolean;
  directoryStatus?: "not_required" | "pending" | "verified";
  last_error?: string | null;
  id: string;
  company_id: string;
  person_id: string;
  engagement_id: string;
  kind: LifecycleInput["kind"];
  status: string;
  version: number;
  effective_at: number;
  package_id: string | null;
  package_revision: number | null;
  definition: LifecycleInput;
  created_by: string;
  updated_at: number;
};
// Technology receives only delivery state. HR-only definition and review
// evidence are deliberately absent from this projection.
export type LifecycleCaseStatus = Pick<
  LifecycleCase,
  | "id"
  | "company_id"
  | "person_id"
  | "engagement_id"
  | "kind"
  | "status"
  | "version"
  | "effective_at"
  | "package_id"
  | "package_revision"
  | "updated_at"
  | "last_error"
  | "directoryStatus"
  | "cutoffEffective"
>;
export function hasLifecycleCaseDetail(
  value: LifecycleCase | LifecycleCaseStatus,
): value is LifecycleCase {
  return "definition" in value && value.definition !== undefined;
}
export type PreparationTask = {
  application_id?: string | null;
  clearance_type?: import("@/lib/people_payroll/retirement").ClearanceKind | null;
  completion_reason?: string;
  document_references?: import("@/lib/people_payroll/retirement").ClearanceReference[] | string;

  due_date?: string | null;
  responsibility?: string;
  privileged?: boolean;
  completed_by?: string | null;
  completed_at?: number | null;
  id: string;
  position_id: string | null;
  case_id: string | null;
  title: string;
  status: string;
  assignee_id: string | null;
  version: number;
  kind: string;
  updated_at: number;
};
export type PeopleLifecycleData = {
  purchases?: import("./purchase-controls").PeoplePurchaseControls;
  clearanceOwners?: {id:string;name:string;payroll:boolean;finance:boolean;equipment:boolean}[];

  roster: {
    id: string;
    name: string;
    engagementId: string;
    itStatus?: "waiting" | "working" | "ready" | "problem";
    gpoDelivery: "none" | "awaiting_identity" | "queued" | "directory_applied" | "blocked";
    positionId: string | null;
    managerPersonId: string | null;
    status: string;
  }[];
  managers: { id: string; name: string }[];
  nodes: { id: string; name: string; kind: string }[];
  packages: AccessPackage[];
  positions: {
    id: string;
    title: string;
    version: number;
    requirements: string;
    package_id: string | null;
    package_revision: number | null;
    gpo: {
      mapped: boolean;
      directoryStatus: "pending" | "directory_applied" | "blocked" | null;
    };
    branch_id: string | null;
    department_id: string | null;
    level_id: string;
    active: number;
  }[];
  cases: (LifecycleCase | LifecycleCaseStatus)[];
  tasks: PreparationTask[];
  settings: {
    version: number;
    itAssigneeId: string | null;
    onboardingTasks: string[];
    offboardingTasks: string[];
  };
  canPrepare: boolean;
  canManage: boolean;
  canReview: boolean;
};
export type LifecyclePreview = {
  recommendation?: {levelName:string|null;currency:string|null;monthlyAmountMinor:number|null;annualDays:number|null;sickDays:number|null};
  legacyCaseToSupersede?: {
    id: string;
    version: number;
    status: string;
    kind: LifecycleInput["kind"];
    effectiveAt: number;
  } | null;
  token: string;
  ready: boolean;
  blockers: string[];
  definition: AccessPackageDefinition | null;
  removed: string[];
  added: string[];
  accounts: string[];
  affectedPositions: string[];
  affectedPeople: string[];
  affectedComputers: string[];
};

export { peopleIamDestination } from "@/lib/people_payroll/people-iam-navigation";
