CREATE TABLE IF NOT EXISTS law.job_queue (
  id bigserial PRIMARY KEY,
  contract_id uuid NOT NULL,
  raw_text text,
  status text NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending', 'running', 'done', 'failed')),
  attempts int NOT NULL DEFAULT 0,
  last_error text,
  enqueued_at timestamptz NOT NULL DEFAULT now(),
  started_at timestamptz,
  finished_at timestamptz
);

CREATE INDEX IF NOT EXISTS law_job_queue_status_idx
  ON law.job_queue(status, enqueued_at)
  WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS law_job_queue_contract_idx
  ON law.job_queue(contract_id);