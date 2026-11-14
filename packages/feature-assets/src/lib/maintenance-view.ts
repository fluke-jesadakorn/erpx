/** Native Maintenance view facts shared by the transport validator and the
 * retained server pages. These are presentation rows, never authority. */
export type MaintenancePurchases = Partial<
  Record<
    string,
    { newWork: boolean; settleExisting: boolean; history: boolean; revision: string }
  >
>;

export type NativeMaintenanceAsset = {
  id: string;
  code: string;
  name: string;
  version: number;
  condition: string;
  lifecycle_status: string;
};

export type NativeMaintenancePerson = { id: string; name: string };

export type NativeMaintenanceNode = {
  id: string;
  kind: string;
  display_name: string;
  branch_id: string | null;
};

export type NativeMaintenanceWork = {
  id: string;
  company_id: string;
  branch_id: string;
  department_id: string | null;
  asset_id: string;
  title: string;
  priority: string;
  status: string;
  owner_id: string;
  scheduled_at: number | null;
  downtime_started_at: number | null;
  downtime_ended_at: number | null;
  blocked_reason: string | null;
  completion_evidence: string | null;
  resulting_condition: string | null;
  version: number;
  created_by: string;
  created_at: number;
  updated_at: number;
  asset_code: string;
  asset_name: string;
  owner_name: string | null;
  owner_eligible: number;
  can_review: number;
};

export type NativeMaintenanceEvent = {
  id: string;
  work_order_id: string;
  version: number;
  action: string;
  actor_id: string;
  reason: string;
  status: string;
  owner_id: string;
  scheduled_at: number | null;
  downtime_started_at: number | null;
  downtime_ended_at: number | null;
  created_at: number;
};

export type NativeMaintenanceWorkspace = {
  items: NativeMaintenanceWork[];
  assets: NativeMaintenanceAsset[];
  people: NativeMaintenancePerson[];
  nodes: NativeMaintenanceNode[];
  canCreate: boolean;
};

export type NativeMaintenanceDetail = NativeMaintenanceWorkspace & {
  work: NativeMaintenanceWork;
  history: NativeMaintenanceEvent[];
  consumptions: Record<string, unknown>[];
  spares: { stock: unknown[]; lots: unknown[] };
};

export type NativeMaintenancePlan = {
  id: string;
  company_id: string;
  branch_id: string;
  department_id: string | null;
  asset_id: string;
  title: string;
  priority: string;
  owner_id: string;
  interval_days: number;
  time_zone: string;
  first_due_local: string;
  next_due_local: string;
  next_due_at: number;
  lead_days: number;
  status: string;
  version: number;
  created_by: string;
  edited_by: string;
  approved_by: string | null;
  approved_at: number | null;
  created_at: number;
  updated_at: number;
  asset_code: string;
  asset_name: string;
  owner_name: string | null;
  owner_eligible: number;
};

export type NativeMaintenancePlanEvent = {
  id: string;
  plan_id: string;
  version: number;
  action: string;
  actor_id: string;
  reason: string;
  created_at: number;
};

export type NativeMaintenanceOccurrence = {
  id: string;
  plan_id: string;
  plan_version: number;
  work_order_id: string;
  due_local: string;
  due_at: number;
  source_asset_version: number;
  generated_by: string;
  generated_at: number;
  status: string;
  work_version: number;
  definition_json: string;
};

export type NativeMaintenancePlanDetail = {
  plan: NativeMaintenancePlan;
  actorId: string;
  canWrite: boolean;
  canReview: boolean;
  assetVersion: number | null;
  people: NativeMaintenancePerson[];
  history: NativeMaintenancePlanEvent[];
  occurrences: NativeMaintenanceOccurrence[];
};

export type NativeMaintenanceContext = {
  userId: string;
  holdingId: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  branchSlug: string | null;
  departmentSlug: string | null;
  assignmentId: string;
  assignmentVersion: number;
  revision: number;
  role: string;
  assignmentScopeKind: string;
  modules: string[];
  permissions: string[];
  expiresAt: number;
};

/** Compatibility views for the retained consoles. The transport validates the
 * native row facts above; these aliases keep the original owner presentation
 * contract (including empty Inventory spares and Commercial controls). */
import type {
  maintenanceDetail,
  maintenanceWorkspace,
} from '@erpx/server-maintenance/public/asset-maintenance';
import type {
  preventivePlanDetail,
  preventivePlans,
} from '@erpx/server-maintenance/public/maintenance-preventive';

export type MaintenanceWorkspaceView = Awaited<ReturnType<typeof maintenanceWorkspace>>;
export type MaintenanceDetailView = Awaited<ReturnType<typeof maintenanceDetail>>;
export type PreventivePlanListView = Awaited<ReturnType<typeof preventivePlans>>;
export type PreventivePlanDetailView = Awaited<ReturnType<typeof preventivePlanDetail>>;
