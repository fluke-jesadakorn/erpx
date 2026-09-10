-- PostgreSQL 18 baseline; fresh installations only. No demo data.
-- Source: final schema of control through 0007_public_beta.sql


-- Shared PostgreSQL functions used by the application-owned SQL contract.
CREATE FUNCTION erpx_json_valid(document text) RETURNS boolean LANGUAGE sql IMMUTABLE AS $$ SELECT document IS JSON $$;
CREATE FUNCTION erpx_json_extract(document text, path text) RETURNS text LANGUAGE sql IMMUTABLE AS $$
 SELECT jsonb_path_query_first(document::jsonb,path::jsonpath) #>> '{}' $$;
CREATE FUNCTION erpx_json_each(document text, path text DEFAULT '$') RETURNS TABLE(key text,value text) LANGUAGE sql IMMUTABLE AS $$
 SELECT (ordinality-1)::text, element #>> '{}' FROM jsonb_array_elements(COALESCE(jsonb_path_query_first(document::jsonb,path::jsonpath),'[]'::jsonb)) WITH ORDINALITY AS a(element,ordinality) $$;
CREATE FUNCTION erpx_json(document text) RETURNS jsonb LANGUAGE sql IMMUTABLE AS $$ SELECT document::jsonb $$;
CREATE FUNCTION erpx_json(document jsonb) RETURNS jsonb LANGUAGE sql IMMUTABLE AS $$ SELECT document $$;
CREATE FUNCTION erpx_json_array_append(state jsonb, element anyelement) RETURNS jsonb LANGUAGE sql IMMUTABLE AS $$ SELECT state || jsonb_build_array(element) $$;
CREATE AGGREGATE erpx_json_array_agg(anyelement) (SFUNC=erpx_json_array_append,STYPE=jsonb,INITCOND='[]');


CREATE TABLE "user" (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  email TEXT NOT NULL UNIQUE,
  "emailVerified" BIGINT NOT NULL DEFAULT 0,
  image TEXT,
  "createdAt" BIGINT NOT NULL,
  "updatedAt" BIGINT NOT NULL
);

CREATE TABLE account (
  id TEXT PRIMARY KEY,
  "accountId" TEXT NOT NULL,
  "providerId" TEXT NOT NULL,
  "userId" TEXT NOT NULL ,
  "accessToken" TEXT,
  "refreshToken" TEXT,
  "idToken" TEXT,
  "accessTokenExpiresAt" BIGINT,
  "refreshTokenExpiresAt" BIGINT,
  scope TEXT,
  password TEXT,
  "createdAt" BIGINT NOT NULL,
  "updatedAt" BIGINT NOT NULL
, issuer TEXT NOT NULL DEFAULT 'local:credential');

CREATE TABLE verification (
  id TEXT PRIMARY KEY,
  identifier TEXT NOT NULL,
  value TEXT NOT NULL,
  "expiresAt" BIGINT NOT NULL,
  "createdAt" BIGINT,
  "updatedAt" BIGINT
);

CREATE TABLE principals (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL UNIQUE ,
  display_name TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','suspended')),
  created_at BIGINT NOT NULL
);

CREATE TABLE holding_bindings (
  holding_id TEXT PRIMARY KEY,
  slug TEXT NOT NULL UNIQUE,
  display_name TEXT NOT NULL,
  write_binding TEXT NOT NULL UNIQUE,
  read_binding TEXT NOT NULL UNIQUE,
  files_binding TEXT NOT NULL UNIQUE,
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','suspended'))
);

CREATE TABLE session (
  id TEXT PRIMARY KEY,
  "expiresAt" TEXT NOT NULL,
  token TEXT NOT NULL UNIQUE,
  "createdAt" TEXT NOT NULL,
  "updatedAt" TEXT NOT NULL,
  "ipAddress" TEXT,
  "userAgent" TEXT,
  "userId" TEXT NOT NULL 
);

CREATE TABLE platform_assignments (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL ,
  role TEXT NOT NULL CHECK(role LIKE 'platform_%'),
  modules_json TEXT NOT NULL CHECK(erpx_json_valid(modules_json)),
  valid_from BIGINT NOT NULL,
  valid_until BIGINT NOT NULL,
  revoked_at BIGINT,
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  CHECK(valid_until > valid_from)
);

CREATE TABLE account_invitations (
  id TEXT PRIMARY KEY,token_hash TEXT NOT NULL UNIQUE,holding_id TEXT NOT NULL,company_id TEXT NOT NULL,
  person_id TEXT NOT NULL,email TEXT NOT NULL,name TEXT NOT NULL,role TEXT NOT NULL,
  scope_kind TEXT NOT NULL CHECK(scope_kind IN ('holding','branch')),branch_id TEXT,
  created_by TEXT NOT NULL,assignment_id TEXT NOT NULL,expires_at BIGINT NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','accepted','revoked')),
  accepted_user_id TEXT,created_at BIGINT NOT NULL,accepted_at BIGINT
);

CREATE TABLE account_invitation_claims (
  invitation_id TEXT PRIMARY KEY ,
  eligible BIGINT NOT NULL CHECK(eligible=1),
  claimed_at BIGINT NOT NULL
);

CREATE TABLE beta_slots (
  holding_id TEXT PRIMARY KEY, owner_user_id TEXT UNIQUE ,
  slug TEXT UNIQUE, company_name TEXT, branch_name TEXT,
  state TEXT NOT NULL DEFAULT 'available' CHECK(state IN ('available','provisioning','active')),
  created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);

CREATE TABLE beta_waitlist (user_id TEXT PRIMARY KEY , created_at BIGINT NOT NULL);

CREATE TABLE beta_invitations (
  id TEXT PRIMARY KEY, holding_id TEXT NOT NULL, email TEXT NOT NULL, branch_id TEXT NOT NULL,
  invited_by TEXT NOT NULL, token_hash TEXT NOT NULL UNIQUE, expires_at BIGINT NOT NULL,
  accepted_user_id TEXT, accepted_at BIGINT, revoked_at BIGINT, created_at BIGINT NOT NULL
);

CREATE TABLE "rateLimit" (id TEXT PRIMARY KEY, key TEXT NOT NULL UNIQUE, count BIGINT NOT NULL, "lastRequest" BIGINT NOT NULL);

CREATE TABLE beta_billing (
  holding_id TEXT PRIMARY KEY, customer_id TEXT UNIQUE, subscription_id TEXT UNIQUE,
  status TEXT NOT NULL DEFAULT 'free_beta', current_period_end BIGINT,
  last_event_at BIGINT NOT NULL DEFAULT 0, updated_at BIGINT NOT NULL
);

CREATE TABLE stripe_inbox (
  id TEXT PRIMARY KEY, event_type TEXT NOT NULL, created_at BIGINT NOT NULL,
  state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','applied','ignored')),
  payload_json TEXT NOT NULL CHECK(erpx_json_valid(payload_json)), applied_at BIGINT
);

ALTER TABLE "account" ADD CONSTRAINT "account_fk_0" FOREIGN KEY ("userId") REFERENCES "user" ("id") ON DELETE CASCADE ON UPDATE NO ACTION;

ALTER TABLE "principals" ADD CONSTRAINT "principals_fk_0" FOREIGN KEY ("user_id") REFERENCES "user" ("id") ON DELETE CASCADE ON UPDATE NO ACTION;

ALTER TABLE "session" ADD CONSTRAINT "session_fk_0" FOREIGN KEY ("userId") REFERENCES "user" ("id") ON DELETE CASCADE ON UPDATE NO ACTION;

ALTER TABLE "platform_assignments" ADD CONSTRAINT "platform_assignments_fk_0" FOREIGN KEY ("user_id") REFERENCES "user" ("id") ON DELETE CASCADE ON UPDATE NO ACTION;

ALTER TABLE "account_invitation_claims" ADD CONSTRAINT "account_invitation_claims_fk_0" FOREIGN KEY ("invitation_id") REFERENCES "account_invitations" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "beta_slots" ADD CONSTRAINT "beta_slots_fk_0" FOREIGN KEY ("owner_user_id") REFERENCES "user" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "beta_waitlist" ADD CONSTRAINT "beta_waitlist_fk_0" FOREIGN KEY ("user_id") REFERENCES "user" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

CREATE UNIQUE INDEX account_provider_idx ON account("providerId", "accountId");

CREATE INDEX account_user_idx ON account("userId");

CREATE INDEX verification_identifier_idx ON verification(identifier);

CREATE UNIQUE INDEX account_issuer_idx
  ON account(issuer, "accountId");

CREATE INDEX session_user_idx ON session("userId");

CREATE INDEX platform_assignment_active_idx
  ON platform_assignments(user_id, valid_from, valid_until);

CREATE INDEX invitation_person_idx ON account_invitations(holding_id,person_id,status);

CREATE INDEX beta_invitation_email ON beta_invitations(email,holding_id);

CREATE UNIQUE INDEX user_email_normalized ON "user" (lower(email));

CREATE TABLE runtime_health(service TEXT PRIMARY KEY,heartbeat_at BIGINT NOT NULL,details_json TEXT NOT NULL DEFAULT '{}' CHECK(erpx_json_valid(details_json))); 
