-- Short marketing-style blurb for each model (max ~140 chars).
-- Surfaced in the user-facing vision model picker and the admin ModelsPane.

ALTER TABLE ai_models
  ADD COLUMN IF NOT EXISTS description TEXT;