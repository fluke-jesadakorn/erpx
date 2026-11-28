import { commercialOwnerProjectionSources as identityAccessProjectionSources } from "@erpx/identity-access-server/public/commercial-owner-projection-sources";
import { evidenceHead } from "./inspection-evidence-source-sql";

/** One parameterized assignment-recipient read. The can-ack existence probe and
 * the recipient count compose this statement; neither keeps an inline SELECT. */
const recipient = `m.company_id=h.company_id
 AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND iam_role_allows(m.role,m.company_id,CASE WHEN h.status IN ('awaiting_release','cancel_pending') THEN 'quality.release' ELSE 'quality.inspect' END)
 AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=h.branch_id))
 AND NOT iam_same_person(m.user_id,h.created_by,h.company_id)
 AND (h.status<>'cancel_pending' OR NOT iam_same_person(m.user_id,h.cancel_requested_by,h.company_id))
 AND (h.status<>'awaiting_release' OR NOT iam_same_person(m.user_id,h.last_inspector,h.company_id))`;

export const qualityAlertRecipientSql = (projection: string, extra = "") =>
  evidenceHead(projection, ` FROM (${identityAccessProjectionSources.assignments}) m WHERE ${recipient}`, extra);
