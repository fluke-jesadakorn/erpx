-- Folio — perm.* schema (v2)
-- Replaces rbac.modules / rbac.permissions / rbac.role_groups /
-- rbac.group_permissions / rbac.module_groups / rbac.tiles / rbac.domains*.
--
-- Four core tables + one ACL table:
--   perm.roles              — named roles (admin, hr_manager, editor, …)
--   perm.permissions        — atom permissions, id = 'domain:subject:verb'
--   perm.role_permissions   — role → permission grants (allow|deny)
--   perm.user_roles         — user → role assignments
--   perm.acl_rules          — object-level rules (row ownership, dept-group)
--
-- Plus perm.audit carry-over target so the new matrix editor writes to
-- a single log table.

CREATE SCHEMA IF NOT EXISTS perm;

-- Enums ----------------------------------------------------------------------

DO $$ BEGIN
  CREATE TYPE perm.effect AS ENUM ('allow', 'deny');
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

-- Roles ----------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS perm.roles (
  id           text PRIMARY KEY,
  display_name text NOT NULL,
  description  text,
  is_system    boolean NOT NULL DEFAULT false,
  sort_order   int  NOT NULL DEFAULT 0,
  created_at   timestamptz NOT NULL DEFAULT now()
);

-- Permissions ----------------------------------------------------------------
-- id grammar: 'domain:subject:verb' (e.g. 'finance:expense:approve')
--   domain ∈ {rbac, user, org, finance, stage, tile, hook, ai, policy, access_request}
--   subject  ∈ {matrix, role, expense, pr, po, …} (lowercase snake)
--   verb     ∈ {read, create, update, delete, approve, reject, settle, disburse,
--                invoke, replay, view, assign, …} (lowercase snake)

CREATE TABLE IF NOT EXISTS perm.permissions (
  id          text PRIMARY KEY,
  domain      text NOT NULL,
  subject     text NOT NULL DEFAULT '*',
  verb        text NOT NULL,
  description text,
  CHECK (id ~ '^[a-z][a-z0-9_]*:[a-z][a-z0-9_]*:[a-z][a-z0-9_]*$')
);
CREATE INDEX IF NOT EXISTS perm_perm_domain_idx ON perm.permissions (domain);
CREATE INDEX IF NOT EXISTS perm_perm_subject_idx ON perm.permissions (subject);

-- Role → Permission ----------------------------------------------------------

CREATE TABLE IF NOT EXISTS perm.role_permissions (
  role_id       text    NOT NULL REFERENCES perm.roles(id) ON DELETE CASCADE,
  permission_id text    NOT NULL REFERENCES perm.permissions(id) ON DELETE CASCADE,
  effect        perm.effect NOT NULL DEFAULT 'allow',
  granted_at    timestamptz NOT NULL DEFAULT now(),
  granted_by    text NOT NULL DEFAULT 'system',
  PRIMARY KEY (role_id, permission_id)
);
CREATE INDEX IF NOT EXISTS perm_rp_role_idx ON perm.role_permissions (role_id);
CREATE INDEX IF NOT EXISTS perm_rp_perm_idx ON perm.role_permissions (permission_id);

-- User → Role ---------------------------------------------------------------

CREATE TABLE IF NOT EXISTS perm.user_roles (
  user_id    integer NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  role_id    text    NOT NULL REFERENCES perm.roles(id) ON DELETE RESTRICT,
  granted_at timestamptz NOT NULL DEFAULT now(),
  granted_by text,
  PRIMARY KEY (user_id, role_id)
);
CREATE INDEX IF NOT EXISTS perm_ur_role_idx ON perm.user_roles (role_id);

-- ACL rules -----------------------------------------------------------------
-- Optional object-level rules. subject_type matches the entity that the
-- permission action targets ('Expense', 'PR', 'Slip', 'ApprovalPolicy', …).
-- owner_field is the column name carrying the user-id of the owner.

CREATE TABLE IF NOT EXISTS perm.acl_rules (
  id              bigserial PRIMARY KEY,
  permission_id   text    NOT NULL REFERENCES perm.permissions(id) ON DELETE CASCADE,
  subject_type    text    NOT NULL,
  owner_field     text    NOT NULL DEFAULT 'owner_id',
  can_assign_to_self   boolean NOT NULL DEFAULT true,
  can_assign_to_group  boolean NOT NULL DEFAULT false,
  created_at      timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS perm_acl_perm_idx ON perm.acl_rules (permission_id);
CREATE INDEX IF NOT EXISTS perm_acl_subj_idx ON perm.acl_rules (subject_type);

-- Audit (carry-over target) -------------------------------------------------

CREATE TABLE IF NOT EXISTS perm.audit (
  id          bigserial PRIMARY KEY,
  kind        text NOT NULL,
  actor       text NOT NULL DEFAULT 'system',
  target      jsonb NOT NULL,
  occurred_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS perm_audit_time_idx ON perm.audit (occurred_at DESC);
CREATE INDEX IF NOT EXISTS perm_audit_kind_idx ON perm.audit (kind);
CREATE INDEX IF NOT EXISTS perm_audit_target_gin ON perm.audit USING gin (target);
