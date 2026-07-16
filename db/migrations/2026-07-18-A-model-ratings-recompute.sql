-- Model ratings recompute: speed + accuracy from last 30 days of ai_invocations.
-- Run via cron / n8n / launchd nightly.

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE IF NOT EXISTS ai.model_ratings (
  model_name text PRIMARY KEY,
  speed int NOT NULL CHECK (speed BETWEEN 1 AND 5),
  accuracy numeric(4,2) NOT NULL CHECK (accuracy BETWEEN 0 AND 5),
  computed_at timestamptz NOT NULL DEFAULT now()
);

CREATE OR REPLACE FUNCTION ai.recompute_model_ratings() RETURNS void AS $$
  INSERT INTO ai.model_ratings (model_name, speed, accuracy, computed_at)
  SELECT
    m.name,
    LEAST(5, GREATEST(1,
      CASE WHEN COALESCE(AVG(i.latency_ms), 0) = 0 THEN 3
           ELSE GREATEST(1, LEAST(5, ROUND(2000.0 / AVG(i.latency_ms))::int))
      END
    )) AS speed,
    LEAST(5.00, GREATEST(0.00,
      ROUND(5.0 * SUM(CASE WHEN i.status='ok' THEN 1 ELSE 0 END)::numeric
            / NULLIF(COUNT(*), 0), 2)
    )) AS accuracy,
    now()
  FROM ai_models m
  JOIN ai_invocations i ON i.model_id = m.id
  WHERE i.created_at >= now() - INTERVAL '30 days'
  GROUP BY m.name
  ON CONFLICT (model_name) DO UPDATE
    SET speed = EXCLUDED.speed,
        accuracy = EXCLUDED.accuracy,
        computed_at = now();
$$ LANGUAGE sql;