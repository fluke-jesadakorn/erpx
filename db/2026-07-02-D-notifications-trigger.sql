-- 2026-07-02-D: domain_events → notifications trigger.
--
-- Refactor: tier 3 trigger projection. Replaces the JS-side fanout in
-- app/src/lib/events.ts with a server-side trigger so the inbox stays
-- in sync without round-trips.
--
-- The trigger function mirrors app/src/lib/notifications/recipients.ts
-- 1:1. Any change to recipients.ts MUST be mirrored here.
--
-- Idempotent: trigger + function are dropped/recreated.
-- Safe to re-run.

BEGIN;

-- D1. Trigger function ---------------------------------------------------------
CREATE OR REPLACE FUNCTION domain_events_fanout_notifications()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
  v_owner        INT;
  v_actor        INT;
  v_supervisor   INT;
  v_uid          INT;
  v_payload      JSONB;
  v_recipients   INT[] := '{}';
  v_domain_id    TEXT;
  v_ref_type     TEXT := NEW.ref_type;
  v_ref_id       BIGINT := NEW.ref_id;
  v_scope_kind   TEXT;
  v_actor_dept   TEXT;
BEGIN
  -- NO_FANOUT filter (mirrors NO_FANOUT_TYPES in recipients.ts:11)
  IF NEW.type = 'policy.updated' THEN
    RETURN NEW;
  END IF;

  -- 1. Resolve ref owner (mirrors lookupRefOwner in recipients.ts:166-190)
  v_owner   := NULL;
  IF v_ref_type = 'expense' AND v_ref_id IS NOT NULL THEN
    SELECT submitter_id  INTO v_owner FROM expenses        WHERE id = v_ref_id;
  ELSIF v_ref_type = 'pr' AND v_ref_id IS NOT NULL THEN
    SELECT requester_id  INTO v_owner FROM purchase_requisitions WHERE id = v_ref_id;
  ELSIF v_ref_type = 'po' AND v_ref_id IS NOT NULL THEN
    SELECT pr.requester_id INTO v_owner
      FROM purchase_orders po JOIN purchase_requisitions pr ON pr.id = po.pr_id
     WHERE po.id = v_ref_id;
  END IF;
  v_actor := NEW.actor_id;

  -- 2. Owner + actor + supervisor (mirrors computeRecipients in recipients.ts:38-69)
  IF NEW.type LIKE 'ceo.override%' THEN
    IF v_owner IS NOT NULL THEN
      v_recipients := array_append(v_recipients, v_owner);
    END IF;
  ELSIF NEW.type LIKE ANY(ARRAY['expense.%','pr.%','po.%']) THEN
    IF v_owner IS NOT NULL THEN
      v_recipients := array_append(v_recipients, v_owner);
    END IF;
    IF v_actor IS NOT NULL AND v_actor <> v_owner THEN
      v_recipients := array_append(v_recipients, v_actor);
    END IF;
    IF v_owner IS NOT NULL THEN
      SELECT reports_to_user_id INTO v_supervisor FROM users WHERE id = v_owner;
      IF v_supervisor IS NOT NULL AND v_supervisor <> v_owner THEN
        v_recipients := array_append(v_recipients, v_supervisor);
      END IF;
    END IF;
  ELSE
    IF v_actor IS NOT NULL THEN
      v_recipients := array_append(v_recipients, v_actor);
    END IF;
  END IF;

  -- 3. Domain-scope expansion (mirrors mapEventToDomain + expandByDomainScope)
  v_domain_id := CASE
    WHEN v_ref_type = 'expense'    THEN 'expenses'
    WHEN v_ref_type = 'pr'         THEN 'pr'
    WHEN v_ref_type = 'po'         THEN 'po'
    WHEN v_ref_type = 'slip'       THEN 'slips'
    WHEN v_ref_type = 'user'       THEN 'users'
    WHEN v_ref_type = 'department' THEN 'departments'
    WHEN v_ref_type = 'audit'      THEN 'audit'
    WHEN v_ref_type = 'ai'         THEN 'ai_settings'
    WHEN v_ref_type = 'notification' THEN 'notifications'
    WHEN NEW.type LIKE 'expense.%'    THEN 'expenses'
    WHEN NEW.type LIKE 'pr.%'        THEN 'pr'
    WHEN NEW.type LIKE 'po.%'        THEN 'po'
    WHEN NEW.type LIKE 'slip.%'      THEN 'slips'
    WHEN NEW.type LIKE 'user.%'      THEN 'users'
    WHEN NEW.type LIKE 'audit.%'     THEN 'audit'
    WHEN NEW.type LIKE 'ai.%'        THEN 'ai_settings'
    WHEN NEW.type LIKE ANY(ARRAY['notification.%','notif.%']) THEN 'notifications'
    ELSE NULL
  END;

  -- Resolve anchor dept (mirrors anchorDept in recipients.ts:96-111)
  v_actor_dept := NULL;
  IF v_owner IS NOT NULL THEN
    SELECT dept_group_id INTO v_actor_dept FROM users WHERE id = v_owner;
  END IF;

  IF v_domain_id IS NOT NULL THEN
    FOR v_uid IN
      SELECT DISTINCT u.id
        FROM rbac.domain_scope ds
        JOIN rbac.roles r  ON r.id = ds.role_id
        JOIN users u       ON u.rbac_role_id = r.id
                           AND u.is_active IS NOT FALSE
       WHERE ds.domain_id = v_domain_id
         AND COALESCE(ds.scope_kind, r.scope_kind) NOT IN ('deny','self')
    LOOP
      v_recipients := array_append(v_recipients, v_uid);
    END LOOP;
  END IF;

  -- Dedupe (mirrors the Set in recipients.ts:40)
  SELECT COALESCE(array_agg(DISTINCT u ORDER BY u), '{}')
    INTO v_recipients FROM unnest(v_recipients) u WHERE u IS NOT NULL;

  IF array_length(v_recipients, 1) IS NULL THEN
    RETURN NEW;
  END IF;

  -- 4. Build enriched payload + bulk insert
  v_payload := COALESCE(NEW.payload, '{}'::jsonb) || jsonb_build_object(
    'actorId',   NEW.actor_id,
    'actorName', (SELECT fullname FROM users WHERE id = NEW.actor_id),
    'severity',  NEW.severity
  );

  INSERT INTO notifications (user_id, type, target_type, target_id, payload_json, created_at)
  SELECT u, NEW.type, v_ref_type, v_ref_id, v_payload, NEW.created_at
    FROM unnest(v_recipients) u;

  RETURN NEW;
END $$;

-- D2. Trigger ------------------------------------------------------------------
DROP TRIGGER IF EXISTS trg_domain_events_fanout ON domain_events;
CREATE TRIGGER trg_domain_events_fanout
  AFTER INSERT ON domain_events
  FOR EACH ROW EXECUTE FUNCTION domain_events_fanout_notifications();

COMMIT;

-- Verify -----------------------------------------------------------------------
SELECT 'domain_events count' AS check, COUNT(*)::text AS n FROM domain_events
UNION ALL
SELECT 'notifications count', COUNT(*)::text FROM notifications;