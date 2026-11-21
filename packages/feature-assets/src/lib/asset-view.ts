/** Native Assets view types shared by the transport validator, server pages and
 * the client console. These are presentation facts only; no authority. */
export type AssetPurchases = Partial<
  Record<
    string,
    { newWork: boolean; settleExisting: boolean; history: boolean; revision: string }
  >
>;

export type NativeAsset = {
  id: string;
  company_id: string;
  branch_id: string;
  department_id: string | null;
  code: string;
  name: string;
  category: string;
  currency: string;
  cost_minor: number;
  acquired_date: string | null;
  warranty_end: string | null;
  finance_asset_id: string | null;
  finance_asset_version: number | null;
  condition: "good" | "degraded" | "unsafe";
  lifecycle_status: "registered" | "in_service" | "maintenance" | "retirement_pending" | "retired";
  placement: string | null;
  custodian_id: string | null;
  custodian_name: string | null;
  retirement_reason: string | null;
  version: number;
  created_by: string;
  created_at: number;
  updated_at: number;
  can_review: boolean;
};

export type NativeAssetEvent = {
  id: string;
  asset_id: string;
  version: number;
  action: string;
  actor_id: string;
  reason: string;
  condition: string;
  lifecycle_status: string;
  placement: string | null;
  custodian_id: string | null;
  created_at: number;
};

export type NativeAssetFinance = {
  id: string;
  asset_number: string;
  name: string | null;
  status: string;
  currency: string;
  cost_minor: number;
  accumulated_depreciation_minor: number | null;
  version: number;
};

export type NativeAssetFinanceCandidate = {
  id: string;
  asset_number: string;
  name: string | null;
  currency: string;
  cost_minor: number;
  version: number;
  status: string;
};

export type NativeAssetPerson = { id: string; name: string };

export type NativeAssetNode = {
  id: string;
  kind: string;
  display_name: string;
  branch_id: string | null;
};

export type NativeAssetMaintenance = {
  id: string;
  title: string;
  status: string;
  priority: string;
  owner_id: string;
  scheduled_at: number | null;
  version: number;
};

export type NativeAssetWorkspace = {
  items: NativeAsset[];
  people: NativeAssetPerson[];
  nodes: NativeAssetNode[];
  financeAssets: NativeAssetFinanceCandidate[];
  canLinkFinance: boolean;
  canCreate: boolean;
  actorId: string;
  purchases?: AssetPurchases;
};

export type NativeAssetDetail = NativeAssetWorkspace & {
  asset: NativeAsset;
  finance: NativeAssetFinance | null;
  history: NativeAssetEvent[];
  maintenance: NativeAssetMaintenance[];
};

export type NativeAssetContext = {
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

/** Presentation-only rows for the retained console. The native transport
 * validates the full row above; these looser shapes keep the existing console
 * fixtures and embeddable callers on their original contract. */
export type AssetViewRow = Record<string, unknown> & {
  id?: string;
  version?: number;
  finance_asset_id?: string | null;
  lifecycle_status?: string;
  can_review?: number | boolean;
  action?: string;
  actor_id?: string;
};

export type AssetWorkspaceView = {
  items: AssetViewRow[];
  people: AssetViewRow[];
  nodes: AssetViewRow[];
  financeAssets: AssetViewRow[];
  canLinkFinance: boolean;
  canCreate: boolean;
  actorId: string;
  purchases?: AssetPurchases;
};

export type AssetDetailView = AssetWorkspaceView & {
  asset: AssetViewRow & { id: string; version: number };
  finance: AssetViewRow | null;
  history: AssetViewRow[];
  maintenance: AssetViewRow[];
};

