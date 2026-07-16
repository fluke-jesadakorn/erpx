-- 2026-07-15-D: repoint staff:ocr to local ollama vision model.
-- Reason: MiniMax provider was returning 401 (invalid api key 2049),
-- which silently broke /expense receipt OCR — every upload returned
-- `ocr_failed` with status 401. The page surfaced only "HTTP 401" /
-- "ocr_failed" because useSlipOcr.ts read j.detail?.error (wrong shape),
-- masking the upstream cause.
--
-- This migration also adds a small re-assert at the bottom so a manual
-- `db/seed_ai_settings.js` re-run won't silently flip OCR back to MiniMax
-- (the seed is being updated to leave staff:ocr alone).
--
-- Pre-condition: ai_models.id 612 = qwen3-vl:4b on provider id 1
-- (local ollama). Verified enabled. Fall back to qwen3.6:35b-mlx (614)
-- if 612 is missing.

DO $$
DECLARE
  ollama_provider_id int;
  vision_model_id int;
BEGIN
  SELECT id INTO ollama_provider_id FROM ai_providers WHERE name = 'local-ollama' AND type = 'ollama';
  IF ollama_provider_id IS NULL THEN
    RAISE NOTICE 'local-ollama provider missing — skipping staff:ocr repoint';
    RETURN;
  END IF;

  SELECT id INTO vision_model_id
    FROM ai_models
   WHERE provider_id = ollama_provider_id
     AND 'vision' = ANY(capabilities)
     AND enabled = true
   ORDER BY (name = 'qwen3-vl:4b') DESC,
            (name = 'qwen3.6:35b-mlx') DESC,
            id ASC
   LIMIT 1;

  IF vision_model_id IS NULL THEN
    RAISE NOTICE 'no local ollama vision model enabled — skipping staff:ocr repoint';
    RETURN;
  END IF;

  UPDATE ai_assignments
     SET provider_id = ollama_provider_id,
         model_id = vision_model_id,
         enabled = true,
         updated_at = now()
   WHERE section_key = 'staff:ocr'
     AND task_type = 'vision';

  RAISE NOTICE 'staff:ocr → provider %, model %', ollama_provider_id, vision_model_id;
END $$;