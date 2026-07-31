-- Preserve bounded, secret-free worker failure context beside the legacy
-- display error. The context is diagnostic state for the outbox row; it is
-- never an event payload and never a source-of-truth business mutation.
ALTER TABLE erpx_control.outbox_events
    ADD COLUMN IF NOT EXISTS last_error_context JSONB;

ALTER TABLE erpx_control.outbox_events
    DROP CONSTRAINT IF EXISTS outbox_last_error_context_object;

ALTER TABLE erpx_control.outbox_events
    ADD CONSTRAINT outbox_last_error_context_object
    CHECK (
        last_error_context IS NULL
        OR jsonb_typeof(last_error_context) = 'object'
    );
