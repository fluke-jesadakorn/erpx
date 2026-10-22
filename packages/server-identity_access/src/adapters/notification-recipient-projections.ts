export function payrollAdvanceNotificationRecipients(){return `SELECT 'payroll-advance-review-'||md5(?||':'||a.user_id),?,a.user_id,'payroll_advance',?,1,'Payroll advance needs an independent decision',?
 FROM assignments a WHERE a.company_id=? AND a.scope_kind='holding' AND a.revoked_at IS NULL AND iam_assignment_active(a.id,?) AND iam_role_allows(a.role,a.company_id,'payroll.approve') AND NOT iam_same_person(a.user_id,?,a.company_id) GROUP BY a.user_id`;}
export function payrollCorrectionNotificationRecipients(){return `SELECT 'payroll-correction-review-'||md5(?||':'||a.user_id),?,a.user_id,'payroll_correction',?,1,'Approved payroll needs an independent correction decision',?
 FROM assignments a WHERE a.company_id=? AND a.scope_kind='holding' AND a.revoked_at IS NULL AND iam_assignment_active(a.id,?) AND iam_role_allows(a.role,a.company_id,'payroll.approve') AND NOT iam_same_person(a.user_id,?,a.company_id) GROUP BY a.user_id`;}
export function accessReviewNotificationRecipients(phase:"due-soon"|"overdue"){const duePredicate=phase==="due-soon"?"c.due_at>? AND c.due_at<=?":"c.due_at<=?";return `SELECT 'iam-access-review-${phase}-'||md5(c.id||':'||c.due_at::text||':'||a.user_id),c.company_id,a.user_id,'iam_access_review',c.id,c.version,?,?
 FROM iam_access_review_campaigns c JOIN assignments a ON a.company_id=c.company_id
 WHERE c.status='open' AND EXISTS(SELECT 1 FROM iam_access_review_items i WHERE i.campaign_id=c.id AND i.decision='pending') AND ${duePredicate}
 AND a.scope_kind='holding' AND a.revoked_at IS NULL AND iam_assignment_active(a.id,?) AND iam_role_allows(a.role,a.company_id,'iam.review')
 GROUP BY c.id,c.company_id,c.version,c.due_at,a.user_id`;}
