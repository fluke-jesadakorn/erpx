-- 1. Allow so_dept_approval in sales_orders.status CHECK
ALTER TABLE sales_orders DROP CONSTRAINT IF EXISTS sales_orders_status_check;
ALTER TABLE sales_orders ADD CONSTRAINT sales_orders_status_check
  CHECK (status IN ('so_draft','so_sales_review','so_dept_approval',
                    'so_credit_check','so_invoiced','so_paid','rejected'));

-- 2. Allow so_dept_approval event kind in waybill_events.kind CHECK
DO $$
DECLARE c text;
BEGIN
  SELECT con.conname INTO c
    FROM pg_constraint con JOIN pg_class rel ON rel.oid = con.conrelid
   WHERE rel.relname = 'waybill_events' AND con.contype = 'c'
     AND pg_get_constraintdef(con.oid) ILIKE '%kind%';
  IF c IS NOT NULL THEN
    EXECUTE format('ALTER TABLE waybill_events DROP CONSTRAINT %I', c);
  END IF;
END $$;
ALTER TABLE waybill_events ADD CONSTRAINT waybill_events_kind_check CHECK (kind IN (
  'submitted','advanced','rejected','corrected','settled','posted-to-gl',
  'slip-attached','signed-off','reversed','authorization-overridden',
  'resubmitted','superseded','created','attached','gl-confirmed',
  'so-submitted','so-reviewed','so-credit-checked','so-auto-approved',
  'so-invoiced','so-rejected','so-paid','so-dept-approved',
  'posted-to-gl-sales-accrual','posted-to-gl-sales-vat',
  'gl-confirmed-accrual','coa-applied'));

-- 3. Grant the new stage perm to manager role
INSERT INTO perm.permissions (id, description)
VALUES ('stage:so_dept_approval:act::allow', 'Act on so_dept_approval stage (top of sales rep''s department approves the order)')
ON CONFLICT (id) DO NOTHING;
INSERT INTO perm.role_permissions (role_id, permission_id, granted_at, granted_by)
SELECT 'manager::3', 'stage:so_dept_approval:act::allow', now(), 1
WHERE NOT EXISTS (
  SELECT 1 FROM perm.role_permissions
   WHERE role_id='manager::3' AND permission_id='stage:so_dept_approval:act::allow'
);

-- 4. Insert bilingual label rows if the i18n table exists
DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_class WHERE relname = 'i18n_strings') THEN
    INSERT INTO i18n_strings (key, en, th, de) VALUES
      ('waybill.stage.soDeptApproval', 'Department Approval', 'อนุมัติจากหัวหน้าแผนก', 'Abteilungsgenehmigung'),
      ('waybill.stage.soDeptApprovalDescription', 'Top of sales rep''s department approves the order', 'หัวหน้าแผนกของเซลล์อนุมัติคำสั่งซื้อ', 'Vorgesetzter genehmigt die Bestellung')
    ON CONFLICT (key) DO NOTHING;
  END IF;
END $$;