-- Durable control-plane replay budget for Stripe dead-letter events.
-- Existing rows begin with no manual replay history; future replays are
-- bounded by the infrastructure policy and this monotonic counter.
ALTER TABLE erpx_control.stripe_event_inbox
    ADD COLUMN IF NOT EXISTS replay_count INTEGER NOT NULL DEFAULT 0;

ALTER TABLE erpx_control.stripe_event_inbox
    DROP CONSTRAINT IF EXISTS stripe_event_inbox_replay_count_nonnegative;

ALTER TABLE erpx_control.stripe_event_inbox
    ADD CONSTRAINT stripe_event_inbox_replay_count_nonnegative
    CHECK (replay_count >= 0);
