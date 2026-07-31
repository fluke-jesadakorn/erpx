export type PlatformStatus = 'active' | 'onboarding' | 'suspended' | 'churned' | 'prospect';
export type SubscriptionState = 'trialing' | 'active' | 'past_due' | 'grace' | 'read_only' | 'canceled';
export type HealthState = 'healthy' | 'degraded' | 'down' | 'unknown';
export type TicketStatus =
  | 'new'
  | 'triaged'
  | 'waiting_for_customer'
  | 'access_requested'
  | 'approved'
  | 'in_progress'
  | 'resolved'
  | 'customer_confirmed'
  | 'closed';
export type TicketSeverity = 'sev1' | 'sev2' | 'sev3' | 'sev4';
export type SupportMode = 'diagnostic_read' | 'tenant_mutation' | 'impersonation';

export interface PlatformCustomerRow {
  id: string;
  account_code: string;
  trading_name: string;
  legal_name: string | null;
  industry: string | null;
  country: string;
  status: PlatformStatus;
  subscription_state: SubscriptionState;
  payment_status: string;
  renewal_at: string | null;
  lifetime_value: string;
  onboarding_stage: string;
  training_status: string;
  data_import_status: string;
  go_live_date: string | null;
  customer_success_owner: string | null;
  database_version: string | null;
  projection_lag_seconds: number;
  worker_health: HealthState;
  storage_bytes: number;
  backup_status: string;
  integration_failures: number;
  api_usage_30d: number;
  last_activity_at: string | null;
  holdings_count: number;
  companies_count: number;
  branches_count: number;
  active_users: number;
  expiring_roles: number;
  module_count: number;
  open_tickets: number;
  support_severity: TicketSeverity | 'none';
  health: HealthState;
}

export interface PlatformHolding {
  id: string;
  name: string;
  owner_contact: string | null;
  security_contact: string | null;
  status: string;
  companies: Array<{
    id: string;
    legal_name: string;
    tax_id: string | null;
    status: string;
    branches: Array<{ id: string; branch_code: string; name: string; status: string }>;
  }>;
}

export interface PlatformEntitlement {
  module_key: string;
  status: string;
  seats: number;
  company_limit: number;
  branch_limit: number;
  included_usage: string;
  active_until: string | null;
}

export interface PlatformTicket {
  id: string;
  ticket_number: string;
  customer_account_id: string;
  module_key: string | null;
  environment: string;
  severity: TicketSeverity;
  category: string;
  affected_users: number;
  description: string;
  diagnostics_consent: boolean;
  assigned_team: string | null;
  assigned_agent: string | null;
  status: TicketStatus;
  created_by: string;
  created_at: string;
  updated_at: string;
}

export interface PlatformRoleAssignment {
  id: string;
  principal_ref: string;
  principal_type: string;
  role_key: string;
  functional_domain: string;
  authority_level: number;
  scope_label: string;
  valid_from: string;
  valid_until: string;
  risk_tier: string;
  grant_reason: string;
  parent_assignment_id: string | null;
  assignment_version: number;
  renewed_from_assignment_id?: string | null;
  effective: boolean;
}

export interface PlatformCustomerDetail {
  customer: PlatformCustomerRow;
  holdings: PlatformHolding[];
  entitlements: PlatformEntitlement[];
  tickets: PlatformTicket[];
  roles: PlatformRoleAssignment[];
  access_audit: Array<{
    action: string;
    module_key: string | null;
    purpose: string;
    correlation_id: string;
    created_at: string;
  }>;
}

export interface PlatformOverview {
  customers: PlatformCustomerRow[];
  tickets: PlatformTicket[];
  roles: PlatformRoleAssignment[];
  metrics: {
    total_customers: number;
    active_customers: number;
    payment_risk: number;
    open_tickets: number;
    roles_expiring_30d: number;
  };
}
