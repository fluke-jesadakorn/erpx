/** IAM owns the current independent review status in the supplied single statement. */
export function technologyPendingReviewProjection(){return "JOIN iam_reviews r ON r.id=g.review_id WHERE g.operation_id=op.id AND r.status='pending'";}
