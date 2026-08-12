-- CRM-owned forecast fields remain operational projections and never become
-- Finance revenue. The group is nullable only for pre-upgrade opportunities.

ALTER TABLE erpx_crm_write.opportunity_guards
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
    ADD CONSTRAINT crm_opportunity_forecast_complete CHECK (
        (account_id IS NULL AND owner_ref IS NULL AND pipeline_category IS NULL
         AND probability_basis_points IS NULL AND expected_close_on IS NULL)
        OR
        (account_id IS NOT NULL AND owner_ref IS NOT NULL AND pipeline_category IS NOT NULL
         AND probability_basis_points IS NOT NULL AND expected_close_on IS NOT NULL)
    ),
    ADD CONSTRAINT crm_opportunity_closed_forecast CHECK (
        (opportunity_stage IN ('won', 'lost')) = (pipeline_category = 'closed')
        OR pipeline_category IS NULL
    );

CREATE INDEX crm_opportunity_scope_forecast
    ON erpx_crm_write.opportunity_guards
        (scope_key, pipeline_category, expected_close_on, opportunity_id);
