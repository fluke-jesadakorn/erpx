-- New Companies acquire the same USD default as pre-existing Companies.
CREATE FUNCTION initialize_company_money_settings() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.kind = 'company' THEN
    INSERT INTO company_money_settings(company_id,default_currency,updated_by,updated_at)
      VALUES(NEW.id,'USD','system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      ON CONFLICT(company_id) DO NOTHING;
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER initialize_company_money_settings_on_insert
  AFTER INSERT ON organization_nodes FOR EACH ROW EXECUTE FUNCTION initialize_company_money_settings();

-- A snapshot may refer only to a rate confirmed within its own Company.
CREATE UNIQUE INDEX company_fx_rates_company_id_id ON company_fx_rates(company_id,id);
ALTER TABLE money_transaction_snapshots ADD CONSTRAINT money_snapshot_company_rate_fk
  FOREIGN KEY(company_id,fx_rate_id) REFERENCES company_fx_rates(company_id,id);
