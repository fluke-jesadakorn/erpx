CREATE TABLE erpx_marketing_write.approval_decision_checkpoints (
    consumer_name text PRIMARY KEY,
    last_global_sequence bigint NOT NULL CHECK (last_global_sequence > 0),
    last_event_id uuid NOT NULL,
    updated_at timestamptz NOT NULL,
    CONSTRAINT approval_decision_consumer_name_chk
        CHECK (consumer_name = 'marketing.operations-approval-decisions.v1')
);
