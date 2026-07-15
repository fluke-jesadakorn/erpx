-- Backfill: copy existing domain_events into per-user notifications inbox.
-- Idempotent: skipped if the same domain_event id is already fanned out
-- for the same recipient + ref (best-effort dedup via row check).
--
-- Recipient routing (same logic as lib/notifications/recipients.ts):
--   ceo.override             -> ref owner (expense submitter / pr requester / po requester via pr)
--   expense.* | pr.* | po.*   -> ref owner + actor (if different) + ref owner's supervisor
--
-- Run manually:
--   PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -f db/backfill_notifications.sql

BEGIN;

WITH recipients AS (
  SELECT
    d.id          AS event_id,
    d.type        AS event_type,
    d.actor_id    AS event_actor,
    d.ref_type    AS event_ref_type,
    d.ref_id      AS event_ref_id,
    d.payload     AS event_payload,
    d.severity    AS event_severity,
    d.created_at  AS event_created_at,
    ref_exp.submitter_id        AS exp_owner,
    ref_pr.requester_id         AS pr_owner,
    ref_po_via_pr.requester_id  AS po_owner,
    -- supervisor of ref owner (fallback actor)
    COALESCE(ref_exp.submitter_id, ref_pr.requester_id, ref_po_via_pr.requester_id, d.actor_id) AS submitter_id
  FROM domain_events d
  LEFT JOIN expenses              ref_exp        ON ref_exp.id = d.ref_id AND d.ref_type = 'expense'
  LEFT JOIN purchase_requisitions ref_pr         ON ref_pr.id  = d.ref_id AND d.ref_type = 'pr'
  LEFT JOIN purchase_orders       ref_po         ON ref_po.id  = d.ref_id AND d.ref_type = 'po'
  LEFT JOIN purchase_requisitions ref_po_via_pr  ON ref_po_via_pr.id = ref_po.pr_id
  WHERE d.type NOT IN ('policy.updated')
),
expanded AS (
  SELECT event_id, event_type, event_actor, event_ref_type, event_ref_id,
         event_payload, event_severity, event_created_at,
         unnest(ARRAY[
           -- ref owner
           COALESCE(exp_owner, pr_owner, po_owner),
           -- actor if distinct
           event_actor
         ]) AS uid
  FROM recipients
  WHERE COALESCE(exp_owner, pr_owner, po_owner, event_actor) IS NOT NULL
),
with_supervisor AS (
  SELECT e.*, sup.reports_to_user_id AS supervisor_id
  FROM expanded e
  LEFT JOIN users sup
    ON sup.id = COALESCE(
         (SELECT u2.id FROM users u2
            WHERE u2.id = (SELECT exp_owner FROM recipients r WHERE r.event_id = e.event_id)),
         (SELECT u3.id FROM users u3
            WHERE u3.id = (SELECT pr_owner FROM recipients r WHERE r.event_id = e.event_id)),
         (SELECT u4.id FROM users u4
            WHERE u4.id = (SELECT po_owner FROM recipients r WHERE r.event_id = e.event_id)),
         e.event_actor
       )
),
final_rows AS (
  SELECT DISTINCT ON (event_id, uid)
    uid AS user_id,
    event_type AS type,
    event_ref_type AS target_type,
    event_ref_id AS target_id,
    jsonb_set(
      jsonb_set(
        jsonb_set(
          COALESCE(event_payload, '{}'::jsonb),
          '{actorId}', to_jsonb(event_actor)
        ),
        '{actorName}', to_jsonb(actor_lookup.fullname)
      ),
      '{severity}', to_jsonb(event_severity)
    ) AS payload_json,
    event_created_at AS created_at
  FROM with_supervisor
  LEFT JOIN LATERAL (
    SELECT uid::int AS candidate
  ) cand ON true
  LEFT JOIN users actor_lookup ON actor_lookup.id = event_actor
  WHERE uid IS NOT NULL
  UNION ALL
  -- supervisor of submitter
  SELECT DISTINCT ON (event_id, sup_id)
    sup_id AS user_id,
    event_type,
    event_ref_type,
    event_ref_id,
    jsonb_set(
      jsonb_set(
        jsonb_set(COALESCE(event_payload, '{}'::jsonb), '{actorId}', to_jsonb(event_actor)),
        '{actorName}', to_jsonb(actor_lookup.fullname)
      ),
      '{severity}', to_jsonb(event_severity)
    ) AS payload_json,
    event_created_at
  FROM (
    SELECT
      d.id AS event_id, d.type AS event_type, d.actor_id AS event_actor,
      d.ref_type AS event_ref_type, d.ref_id AS event_ref_id,
      d.payload AS event_payload, d.severity AS event_severity,
      d.created_at AS event_created_at,
      sub.reports_to_user_id AS sup_id
    FROM domain_events d
    LEFT JOIN expenses              e ON e.id = d.ref_id AND d.ref_type = 'expense'
    LEFT JOIN purchase_requisitions pr ON pr.id = d.ref_id AND d.ref_type = 'pr'
    LEFT JOIN purchase_orders       po ON po.id = d.ref_id AND d.ref_type = 'po'
    LEFT JOIN purchase_requisitions po_pr ON po_pr.id = po.pr_id
    LEFT JOIN users sub
      ON sub.id = COALESCE(e.submitter_id, pr.requester_id, po_pr.requester_id)
    WHERE d.type NOT IN ('policy.updated')
      AND sub.reports_to_user_id IS NOT NULL
      AND sub.reports_to_user_id <> sub.id
  ) sup_src
  LEFT JOIN users actor_lookup ON actor_lookup.id = event_actor
)
INSERT INTO notifications (user_id, type, target_type, target_id, payload_json, created_at)
SELECT user_id, type, target_type, target_id, payload_json, created_at
FROM final_rows
WHERE NOT EXISTS (
  SELECT 1 FROM notifications n
  WHERE n.user_id   = final_rows.user_id
    AND n.type      = final_rows.type
    AND n.target_id IS NOT DISTINCT FROM final_rows.target_id
    AND n.created_at = final_rows.created_at
);

COMMIT;

-- Summary
SELECT 'backfilled' AS step, COUNT(*) AS rows FROM notifications;