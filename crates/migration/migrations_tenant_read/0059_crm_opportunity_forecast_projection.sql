-- Forecast cockpit fields are disposable and rebuild from CRM snapshots.

ALTER TABLE erpx_crm_read.opportunity_projection
    ADD COLUMN account_id UUID,
    ADD COLUMN owner_ref TEXT,
    ADD COLUMN pipeline_category TEXT CHECK (
        pipeline_category IN ('pipeline', 'best_case', 'commit', 'closed')
    ),
    ADD COLUMN probability_basis_points INTEGER CHECK (
        probability_basis_points BETWEEN 0 AND 10000
    ),
    ADD COLUMN expected_close_on DATE,
    ADD COLUMN originating_campaign_id UUID,
    ADD CONSTRAINT crm_read_opportunity_forecast_complete CHECK (
        (account_id IS NULL AND owner_ref IS NULL AND pipeline_category IS NULL
         AND probability_basis_points IS NULL AND expected_close_on IS NULL)
        OR
        (account_id IS NOT NULL AND owner_ref IS NOT NULL AND pipeline_category IS NOT NULL
         AND probability_basis_points IS NOT NULL AND expected_close_on IS NOT NULL)
    );

CREATE INDEX crm_read_opportunity_forecast_cockpit
    ON erpx_crm_read.opportunity_projection
        (generation, scope_key, pipeline_category, expected_close_on, opportunity_id);
