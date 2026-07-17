-- Provider health — record of the last test attempt per provider

CREATE TABLE IF NOT EXISTS ai_provider_health (
  provider_id INT NOT NULL REFERENCES ai_providers(id) ON DELETE CASCADE,
  ok BOOLEAN NOT NULL,
  model_count INT,
  latency_ms INT,
  error TEXT,
  checked_at TIMESTAMP NOT NULL DEFAULT now(),
  PRIMARY KEY (provider_id)
);
