BEGIN;

-- ERPX control-plane primitives. Tenant business data remains in Folio's
-- operational schemas; these tables contain platform metadata, approvals,
-- entitlements, support capabilities, and access assignments only.
CREATE SCHEMA IF NOT EXISTS platform;

CREATE TABLE IF NOT EXISTS platform.catalog_revisions (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  revision_number integer NOT NULL UNIQUE,
  status text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'published', 'retired')),
  currency text NOT NULL DEFAULT 'THB',
  effective_at timestamptz,
  published_by text,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS platform.catalog_items (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  revision_id uuid NOT NULL REFERENCES platform.catalog_revisions(id) ON DELETE CASCADE,
  item_key text NOT NULL,
  display_name text NOT NULL,
  billing_model text NOT NULL CHECK (billing_model IN ('flat', 'per_seat', 'per_company', 'per_branch', 'one_time', 'metered')),
  unit_amount numeric(14,2) NOT NULL DEFAULT 0 CHECK (unit_amount >= 0),
  interval text CHECK (interval IN ('month', 'year') OR interval IS NULL),
  included_quantity integer NOT NULL DEFAULT 0 CHECK (included_quantity >= 0),
  stripe_product_id text,
  stripe_price_id text,
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (revision_id, item_key)
);

CREATE TABLE IF NOT EXISTS platform.customer_accounts (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  legacy_customer_id integer UNIQUE REFERENCES folio.customers(id) ON DELETE SET NULL,
  account_code text NOT NULL UNIQUE,
  trading_name text NOT NULL,
  legal_name text,
  registration_number text,
  tax_id text,
  industry text,
  country text NOT NULL DEFAULT 'TH',
  locale text NOT NULL DEFAULT 'th',
  billing_address text,
  domains text[] NOT NULL DEFAULT '{}',
  logo_url text,
  status text NOT NULL DEFAULT 'active' CHECK (status IN ('prospect', 'onboarding', 'active', 'suspended', 'churned')),
  subscription_state text NOT NULL DEFAULT 'trialing' CHECK (subscription_state IN ('trialing', 'active', 'past_due', 'grace', 'read_only', 'canceled')),
  stripe_customer_id text,
  stripe_subscription_id text,
  payment_status text NOT NULL DEFAULT 'trialing',
  trial_ends_at timestamptz,
  renewal_at timestamptz,
  lifetime_value numeric(16,2) NOT NULL DEFAULT 0 CHECK (lifetime_value >= 0),
  onboarding_stage text NOT NULL DEFAULT 'discovery',
  implementation_tasks jsonb NOT NULL DEFAULT '[]',
  training_status text NOT NULL DEFAULT 'not_started',
  data_import_status text NOT NULL DEFAULT 'not_started',
  go_live_date date,
  customer_success_owner text,
  database_version text,
  projection_lag_seconds numeric(12,2) NOT NULL DEFAULT 0,
  worker_health text NOT NULL DEFAULT 'healthy' CHECK (worker_health IN ('healthy', 'degraded', 'down', 'unknown')),
  storage_bytes bigint NOT NULL DEFAULT 0 CHECK (storage_bytes >= 0),
  backup_status text NOT NULL DEFAULT 'unknown',
  integration_failures integer NOT NULL DEFAULT 0 CHECK (integration_failures >= 0),
  api_usage_30d bigint NOT NULL DEFAULT 0 CHECK (api_usage_30d >= 0),
  last_activity_at timestamptz,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS platform.holdings (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  customer_account_id uuid NOT NULL REFERENCES platform.customer_accounts(id) ON DELETE CASCADE,
  name text NOT NULL,
  owner_contact text,
  security_contact text,
  status text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'suspended', 'archived')),
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (customer_account_id, name)
);

CREATE TABLE IF NOT EXISTS platform.companies (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  holding_id uuid NOT NULL REFERENCES platform.holdings(id) ON DELETE CASCADE,
  legal_name text NOT NULL,
  registration_number text,
  tax_id text,
  country text NOT NULL DEFAULT 'TH',
  status text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'inactive')),
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (holding_id, legal_name)
);

CREATE TABLE IF NOT EXISTS platform.branches (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  company_id uuid NOT NULL REFERENCES platform.companies(id) ON DELETE CASCADE,
  branch_code text NOT NULL,
  name text NOT NULL,
  status text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'inactive')),
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (company_id, branch_code)
);

CREATE TABLE IF NOT EXISTS platform.customer_users (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  customer_account_id uuid NOT NULL REFERENCES platform.customer_accounts(id) ON DELETE CASCADE,
  external_user_ref text NOT NULL,
  display_name text NOT NULL,
  email text,
  status text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'inactive', 'invited')),
  last_activity_at timestamptz,
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (customer_account_id, external_user_ref)
);

CREATE TABLE IF NOT EXISTS platform.entitlements (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  customer_account_id uuid NOT NULL REFERENCES platform.customer_accounts(id) ON DELETE CASCADE,
  module_key text NOT NULL,
  status text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'scheduled', 'paused', 'expired')),
  seats integer NOT NULL DEFAULT 0 CHECK (seats >= 0),
  company_limit integer NOT NULL DEFAULT 0 CHECK (company_limit >= 0),
  branch_limit integer NOT NULL DEFAULT 0 CHECK (branch_limit >= 0),
  included_usage numeric(16,2) NOT NULL DEFAULT 0,
  usage jsonb NOT NULL DEFAULT '{}',
  feature_flags jsonb NOT NULL DEFAULT '{}',
  stripe_entitlement_id text,
  active_from timestamptz NOT NULL DEFAULT now(),
  active_until timestamptz,
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (customer_account_id, module_key)
);

CREATE TABLE IF NOT EXISTS platform.support_tickets (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  ticket_number text NOT NULL UNIQUE,
  customer_account_id uuid NOT NULL REFERENCES platform.customer_accounts(id) ON DELETE RESTRICT,
  holding_id uuid REFERENCES platform.holdings(id) ON DELETE SET NULL,
  company_id uuid REFERENCES platform.companies(id) ON DELETE SET NULL,
  branch_id uuid REFERENCES platform.branches(id) ON DELETE SET NULL,
  module_key text,
  environment text NOT NULL DEFAULT 'production' CHECK (environment IN ('production', 'staging', 'sandbox')),
  severity text NOT NULL DEFAULT 'sev3' CHECK (severity IN ('sev1', 'sev2', 'sev3', 'sev4')),
  category text NOT NULL DEFAULT 'question',
  affected_users integer NOT NULL DEFAULT 0 CHECK (affected_users >= 0),
  description text NOT NULL,
  diagnostics_consent boolean NOT NULL DEFAULT false,
  sla_due_at timestamptz,
  assigned_team text,
  assigned_agent text,
  status text NOT NULL DEFAULT 'new' CHECK (status IN ('new', 'triaged', 'waiting_for_customer', 'access_requested', 'approved', 'in_progress', 'resolved', 'customer_confirmed', 'closed')),
  timeline jsonb NOT NULL DEFAULT '[]',
  internal_notes jsonb NOT NULL DEFAULT '[]',
  created_by text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  closed_at timestamptz
);

CREATE TABLE IF NOT EXISTS platform.support_sessions (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  ticket_id uuid NOT NULL REFERENCES platform.support_tickets(id) ON DELETE RESTRICT,
  customer_account_id uuid NOT NULL REFERENCES platform.customer_accounts(id) ON DELETE RESTRICT,
  agent_user_id integer REFERENCES folio.users(id) ON DELETE SET NULL,
  agent_name text NOT NULL,
  represented_tenant_user text,
  mode text NOT NULL CHECK (mode IN ('diagnostic_read', 'tenant_mutation', 'impersonation')),
  permissions text[] NOT NULL DEFAULT '{}',
  scope jsonb NOT NULL DEFAULT '{}',
  reason text NOT NULL,
  requested_duration_minutes integer NOT NULL CHECK (requested_duration_minutes > 0),
  approved_by text,
  approved_at timestamptz,
  requested_at timestamptz NOT NULL DEFAULT now(),
  expires_at timestamptz NOT NULL,
  revoked_at timestamptz,
  status text NOT NULL DEFAULT 'requested' CHECK (status IN ('requested', 'approved', 'rejected', 'active', 'expired', 'revoked', 'closed')),
  CHECK (expires_at > requested_at),
  CHECK (mode = 'diagnostic_read' AND requested_duration_minutes <= 480 OR mode <> 'diagnostic_read' AND requested_duration_minutes <= 120),
  CHECK (mode <> 'impersonation' OR represented_tenant_user IS NOT NULL)
);

CREATE TABLE IF NOT EXISTS platform.support_access_audit (
  id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  session_id uuid REFERENCES platform.support_sessions(id) ON DELETE SET NULL,
  ticket_id uuid REFERENCES platform.support_tickets(id) ON DELETE SET NULL,
  customer_account_id uuid NOT NULL REFERENCES platform.customer_accounts(id) ON DELETE RESTRICT,
  platform_actor_id integer REFERENCES folio.users(id) ON DELETE SET NULL,
  represented_tenant_user text,
  action text NOT NULL,
  module_key text,
  company_id uuid,
  branch_id uuid,
  purpose text NOT NULL,
  correlation_id text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS platform.role_assignments (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  principal_ref text NOT NULL,
  principal_type text NOT NULL CHECK (principal_type IN ('platform_user', 'tenant_user', 'service')),
  role_key text NOT NULL,
  permissions text[] NOT NULL DEFAULT '{}',
  functional_domain text NOT NULL,
  authority_level integer NOT NULL DEFAULT 99,
  holding_id uuid REFERENCES platform.holdings(id) ON DELETE CASCADE,
  company_id uuid REFERENCES platform.companies(id) ON DELETE CASCADE,
  branch_id uuid REFERENCES platform.branches(id) ON DELETE CASCADE,
  subtree_scope jsonb NOT NULL DEFAULT '{}',
  valid_from timestamptz NOT NULL,
  valid_until timestamptz NOT NULL,
  timezone text NOT NULL DEFAULT 'UTC',
  risk_tier text NOT NULL DEFAULT 'standard' CHECK (risk_tier IN ('platform_root', 'administrator', 'high', 'standard', 'contractor', 'support', 'emergency')),
  grant_reason text NOT NULL,
  ticket_reference text,
  contract_reference text,
  grantor text NOT NULL,
  approver text,
  renewal_reviewer text,
  parent_assignment_id uuid REFERENCES platform.role_assignments(id) ON DELETE RESTRICT,
  assignment_version integer NOT NULL DEFAULT 1 CHECK (assignment_version > 0),
  revoked_at timestamptz,
  created_at timestamptz NOT NULL DEFAULT now(),
  CHECK (valid_until > valid_from)
);

CREATE INDEX IF NOT EXISTS platform_customer_status_idx ON platform.customer_accounts(status, subscription_state);
CREATE INDEX IF NOT EXISTS platform_customer_renewal_idx ON platform.customer_accounts(renewal_at);
CREATE INDEX IF NOT EXISTS platform_ticket_queue_idx ON platform.support_tickets(status, severity, updated_at DESC);
CREATE INDEX IF NOT EXISTS platform_session_expiry_idx ON platform.support_sessions(status, expires_at);
CREATE INDEX IF NOT EXISTS platform_role_expiry_idx ON platform.role_assignments(valid_until, revoked_at);
CREATE INDEX IF NOT EXISTS platform_role_scope_idx ON platform.role_assignments(holding_id, company_id, branch_id);

-- Seed the control plane from the legacy sales-customer catalog. Further
-- customer metadata is intentionally owned by platform.customer_accounts.
INSERT INTO platform.customer_accounts (
  legacy_customer_id, account_code, trading_name, legal_name, tax_id,
  billing_address, status, subscription_state, payment_status, last_activity_at
)
SELECT c.id, c.code, c.name, c.name, c.tax_id, c.billing_address,
       CASE WHEN c.is_active THEN 'active' ELSE 'suspended' END,
       'trialing', 'trialing', c.updated_at
  FROM folio.customers c
 ON CONFLICT (legacy_customer_id) DO UPDATE
       SET trading_name = EXCLUDED.trading_name,
           legal_name = COALESCE(platform.customer_accounts.legal_name, EXCLUDED.legal_name),
           tax_id = COALESCE(platform.customer_accounts.tax_id, EXCLUDED.tax_id),
           billing_address = COALESCE(platform.customer_accounts.billing_address, EXCLUDED.billing_address),
           updated_at = now();

INSERT INTO perm.permissions (id, description)
VALUES ('platform:customer:view::allow', 'View the ERPX Platform Customer Console'),
       ('platform:customer:manage::allow', 'Manage ERPX control-plane customer metadata'),
       ('platform:support:manage::allow', 'Create and manage support tickets and capability requests'),
       ('platform:rbac:review::allow', 'Review expiring ERPX role assignments')
ON CONFLICT (id) DO NOTHING;

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id, granted_by, significance)
SELECT r.id, r.kind, p.permission_id, 'erpx-control-plane', TRUE
  FROM perm.roles r
 CROSS JOIN (VALUES
   ('platform:customer:view::allow'),
   ('platform:customer:manage::allow'),
   ('platform:support:manage::allow'),
   ('platform:rbac:review::allow')
 ) p(permission_id)
 WHERE r.id IN ('it_manager', 'it_supervisor')
 ON CONFLICT (role_id, permission_id) DO NOTHING;

COMMIT;
