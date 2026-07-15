-- AI Settings: providers, models, staff (agents), section assignments, invocation audit
-- Encrypted API keys via pgcrypto using ENCRYPTION_KEY env var (single-source key)

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- Encryption helpers (key passed by app at call time via ai_encrypt/ai_decrypt SQL funcs)
-- We store as bytea so the key never leaves the DB connection's process.
CREATE OR REPLACE FUNCTION ai_encrypt(plain TEXT, key TEXT) RETURNS BYTEA AS $$
  SELECT pgp_sym_encrypt(plain, key)
$$ LANGUAGE SQL;

CREATE OR REPLACE FUNCTION ai_decrypt(cipher BYTEA, key TEXT) RETURNS TEXT AS $$
  SELECT pgp_sym_decrypt(cipher, key)
$$ LANGUAGE SQL;

CREATE TABLE IF NOT EXISTS ai_providers (
  id SERIAL PRIMARY KEY,
  name TEXT UNIQUE NOT NULL,
  type VARCHAR(20) NOT NULL CHECK (type IN ('ollama','openai_compat','minimax')),
  base_url TEXT NOT NULL,
  api_key_enc BYTEA,
  enabled BOOLEAN NOT NULL DEFAULT true,
  preset VARCHAR(40),
  notes TEXT,
  created_at TIMESTAMP NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS ai_models (
  id SERIAL PRIMARY KEY,
  provider_id INT NOT NULL REFERENCES ai_providers(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  capabilities TEXT[] NOT NULL DEFAULT '{}',
  context_window INT,
  defaults_json JSONB NOT NULL DEFAULT '{}'::jsonb,
  enabled BOOLEAN NOT NULL DEFAULT true,
  created_at TIMESTAMP NOT NULL DEFAULT NOW(),
  UNIQUE(provider_id, name)
);
CREATE INDEX IF NOT EXISTS idx_ai_models_provider ON ai_models(provider_id);

CREATE TABLE IF NOT EXISTS ai_staff (
  id SERIAL PRIMARY KEY,
  name TEXT NOT NULL,
  role_label TEXT,
  description TEXT,
  system_prompt TEXT NOT NULL,
  capabilities TEXT[] NOT NULL DEFAULT '{}',
  default_provider_id INT REFERENCES ai_providers(id) ON DELETE SET NULL,
  default_model_id INT REFERENCES ai_models(id) ON DELETE SET NULL,
  enabled BOOLEAN NOT NULL DEFAULT true,
  created_at TIMESTAMP NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS ai_assignments (
  id SERIAL PRIMARY KEY,
  section_key TEXT NOT NULL,
  task_type VARCHAR(20) NOT NULL CHECK (task_type IN ('embed','chat','vision')),
  provider_id INT REFERENCES ai_providers(id) ON DELETE SET NULL,
  model_id INT REFERENCES ai_models(id) ON DELETE SET NULL,
  staff_id INT REFERENCES ai_staff(id) ON DELETE SET NULL,
  params_json JSONB NOT NULL DEFAULT '{}'::jsonb,
  priority INT NOT NULL DEFAULT 100,
  enabled BOOLEAN NOT NULL DEFAULT true,
  created_at TIMESTAMP NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_ai_assignments_section ON ai_assignments(section_key, task_type, priority);
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname = 'ai_assignments_unique'
  ) THEN
    ALTER TABLE ai_assignments
      ADD CONSTRAINT ai_assignments_unique UNIQUE (section_key, task_type, priority);
  END IF;
END$$;

CREATE TABLE IF NOT EXISTS ai_invocations (
  id BIGSERIAL PRIMARY KEY,
  staff_id INT REFERENCES ai_staff(id) ON DELETE SET NULL,
  section_key TEXT,
  task_type VARCHAR(20),
  provider_id INT REFERENCES ai_providers(id) ON DELETE SET NULL,
  model_id INT REFERENCES ai_models(id) ON DELETE SET NULL,
  prompt_tokens INT,
  response_tokens INT,
  latency_ms INT,
  status VARCHAR(20) NOT NULL,
  error TEXT,
  prompt_excerpt TEXT,
  response_excerpt TEXT,
  actor_id INT REFERENCES users(id) ON DELETE SET NULL,
  created_at TIMESTAMP NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_ai_invocations_section ON ai_invocations(section_key, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_ai_invocations_staff ON ai_invocations(staff_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_ai_invocations_created ON ai_invocations(created_at DESC);