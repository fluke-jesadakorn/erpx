-- Company-wide presentation preferences and human-confirmed USD quotes.
-- Existing accounting bases and historical two-decimal amounts are untouched.
CREATE TABLE company_money_settings (
  company_id TEXT PRIMARY KEY,
  default_currency TEXT NOT NULL DEFAULT 'USD' CHECK (default_currency ~ '^[A-Z]{3}$'),
  version BIGINT NOT NULL DEFAULT 1,
  updated_by TEXT NOT NULL,
  updated_at BIGINT NOT NULL
);
INSERT INTO company_money_settings(company_id,default_currency,updated_by,updated_at)
 SELECT id,'USD','system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint
 FROM organization_nodes WHERE kind='company' ON CONFLICT DO NOTHING;

CREATE TABLE company_currency_scales (
  company_id TEXT NOT NULL,
  currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
  scale SMALLINT NOT NULL CHECK (scale BETWEEN 0 AND 12),
  configured_by TEXT NOT NULL,
  configured_at BIGINT NOT NULL,
  PRIMARY KEY (company_id, currency)
);

CREATE TABLE user_currency_preferences (
  company_id TEXT NOT NULL,
  user_id TEXT NOT NULL,
  display_currency TEXT NOT NULL CHECK (display_currency ~ '^[A-Z]{3}$'),
  version BIGINT NOT NULL DEFAULT 1,
  updated_at BIGINT NOT NULL,
  PRIMARY KEY (company_id, user_id)
);

CREATE TABLE company_fx_rates (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$' AND currency <> 'USD'),
  effective_date DATE NOT NULL,
  -- Currency units for exactly one USD. Decimal, never a JavaScript float.
  currency_per_usd NUMERIC(38,18) NOT NULL CHECK (currency_per_usd > 0),
  confirmed_by TEXT NOT NULL,
  confirmed_at BIGINT NOT NULL,
  note TEXT NOT NULL DEFAULT '',
  UNIQUE (company_id, currency, effective_date, confirmed_at, id)
);
CREATE INDEX company_fx_rates_lookup ON company_fx_rates(company_id,currency,effective_date DESC,confirmed_at DESC);

CREATE TABLE money_transaction_snapshots (
  company_id TEXT NOT NULL,
  source_type TEXT NOT NULL,
  source_id TEXT NOT NULL,
  amount_kind TEXT NOT NULL,
  transaction_date DATE NOT NULL,
  currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
  currency_scale SMALLINT NOT NULL CHECK (currency_scale BETWEEN 0 AND 12),
  amount_units NUMERIC(38,0) NOT NULL,
  usd_units NUMERIC(38,0) NOT NULL,
  fx_rate_id TEXT REFERENCES company_fx_rates(id),
  used_latest_fallback BOOLEAN NOT NULL DEFAULT false,
  captured_at BIGINT NOT NULL,
  PRIMARY KEY (company_id,source_type,source_id,amount_kind)
);

CREATE FUNCTION money_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'MONEY_HISTORY_IMMUTABLE' USING ERRCODE='23514';
END $$;
CREATE TRIGGER company_fx_rate_immutable BEFORE UPDATE OR DELETE ON company_fx_rates FOR EACH ROW EXECUTE FUNCTION money_immutable();
CREATE TRIGGER money_transaction_snapshot_immutable BEFORE UPDATE OR DELETE ON money_transaction_snapshots FOR EACH ROW EXECUTE FUNCTION money_immutable();

INSERT INTO work_cache_revisions(table_name) VALUES
 ('company_money_settings'),('company_currency_scales'),('user_currency_preferences'),
 ('company_fx_rates'),('money_transaction_snapshots') ON CONFLICT DO NOTHING;

DO $$
DECLARE migration_role TEXT := current_user;
DECLARE app_role TEXT;
BEGIN
 IF migration_role !~ '_migration$' THEN RETURN; END IF;
 app_role := regexp_replace(migration_role, '_migration$', '_app');
 EXECUTE format('GRANT SELECT, INSERT, UPDATE ON company_money_settings,company_currency_scales,user_currency_preferences TO %I',app_role);
 EXECUTE format('GRANT SELECT, INSERT ON company_fx_rates,money_transaction_snapshots TO %I',app_role);
END $$;
