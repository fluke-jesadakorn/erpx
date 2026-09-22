INSERT INTO account_invitation_delivery_jobs(
  id,invitation_id,holding_id,company_id,state,next_attempt_at,created_at,updated_at
)
SELECT
  'account-invite-delivery-' || id,id,holding_id,company_id,
  'awaiting_approval',created_at,created_at,created_at
FROM account_invitations
WHERE status='pending'
ON CONFLICT(invitation_id) DO NOTHING;
