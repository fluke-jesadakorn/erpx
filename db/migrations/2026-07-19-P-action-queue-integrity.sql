BEGIN;

UPDATE folio.waybills w
   SET submitter_id = COALESCE(w.submitter_id, e.submitter_id),
       vendor_name = COALESCE(w.vendor_name, e.vendor_name),
       total_amount = COALESCE(w.total_amount, e.total_amount)
  FROM folio.expenses e
 WHERE w.origin = 'expense' AND e.id = w.origin_id;

UPDATE folio.waybills w
   SET submitter_id = COALESCE(w.submitter_id, pr.requester_id),
       vendor_name = COALESCE(w.vendor_name, pr.vendor_name),
       total_amount = COALESCE(w.total_amount, pr.total_estimate),
       currency = COALESCE(w.currency, pr.currency, 'THB')
  FROM folio.purchase_requisitions pr
 WHERE w.origin = 'pr' AND pr.id = w.origin_id;

UPDATE folio.waybills w
   SET submitter_id = COALESCE(w.submitter_id, pr.requester_id),
       vendor_name = COALESCE(w.vendor_name, po.vendor_name),
       total_amount = COALESCE(w.total_amount, po.total_amount),
       currency = COALESCE(w.currency, po.currency, 'THB')
  FROM folio.purchase_orders po
  LEFT JOIN folio.purchase_requisitions pr ON pr.id = po.pr_id
 WHERE w.origin = 'po' AND po.id = w.origin_id;

UPDATE folio.waybills w
   SET submitter_id = COALESCE(w.submitter_id, so.sales_rep_id),
       total_amount = COALESCE(w.total_amount, so.total_amount),
       currency = COALESCE(w.currency, so.currency, 'THB')
  FROM folio.sales_orders so
 WHERE w.origin = 'so' AND so.id = w.origin_id;

UPDATE folio.waybills w
   SET submitter_id = COALESCE(w.submitter_id, hl.employee_id)
  FROM folio.hr_leave hl
 WHERE w.origin = 'hr_leave' AND hl.waybill_id = w.id;

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM folio.waybills WHERE submitter_id IS NULL) THEN
    RAISE EXCEPTION 'Cannot enforce waybills.submitter_id: unresolved Waybill owner exists';
  END IF;
END $$;

ALTER TABLE folio.waybills
  ALTER COLUMN submitter_id SET NOT NULL;

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id, granted_by, significance)
SELECT DISTINCT rp.role_id, rp.role_kind, 'tile:inbox:view::allow', 'action-queue-integrity', FALSE
  FROM perm.role_permissions rp
 WHERE rp.permission_id LIKE 'stage:%:act%::allow'
ON CONFLICT (role_id, permission_id) DO NOTHING;

COMMIT;
