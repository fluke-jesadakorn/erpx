import {collaborationNotificationSnapshotSources as qualitySnapshotSources} from '@erpx/server-quality/public/collaboration-notification-snapshot-sources';
import {collaborationNotificationSnapshotSources as identityaccessSnapshotSources} from '@erpx/server-identity_access/public/collaboration-notification-snapshot-sources';
const qualitySources=qualitySnapshotSources();
const identityaccessSources=identityaccessSnapshotSources();
import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {scheduledFeaturePredicate} from '@erpx/server-pms/public/hospitality-rollout';
const eligible=`a.company_id=h.company_id AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=h.branch_id)) AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,CASE WHEN h.status IN ('awaiting_release','cancel_pending') THEN 'quality.release' ELSE 'quality.inspect' END) AND NOT iam_same_person(a.user_id,h.created_by,h.company_id) AND (h.status<>'cancel_pending' OR NOT iam_same_person(a.user_id,h.cancel_requested_by,h.company_id)) AND (h.status<>'awaiting_release' OR NOT iam_same_person(a.user_id,h.last_inspector,h.company_id))`;


const terminal = "('released','cancelled','disposed')";
// Assignment scope and the required quality authority are evaluated from the
// current hold state on every sweep and read. No recipient routing is copied
// into the alert, so revocation and role changes take effect immediately.
const recipient = `m.company_id=h.company_id
 AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND iam_role_allows(m.role,m.company_id,CASE WHEN h.status IN ('awaiting_release','cancel_pending') THEN 'quality.release' ELSE 'quality.inspect' END)
 AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=h.branch_id))
 AND NOT iam_same_person(m.user_id,h.created_by,h.company_id)
 AND (h.status<>'cancel_pending' OR NOT iam_same_person(m.user_id,h.cancel_requested_by,h.company_id))
 AND (h.status<>'awaiting_release' OR NOT iam_same_person(m.user_id,h.last_inspector,h.company_id))`;


export function prepareQualityNotification(db:SqlDatabase,key:string,id:string){return db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT DISTINCT ?||'-'||a.user_id,h.company_id,a.user_id,'quality_hold',h.id,h.version,'Quality handoff: review the source record',h.updated_at ${qualitySources.quality_notifications_inventory_quality_holds_h_from} ${identityaccessSources.quality_notifications_assignments_a_join} ON ((${eligible} AND h.status NOT IN ('released','cancelled','disposed')) OR (h.status IN ('released','cancelled','disposed') AND a.company_id=h.company_id AND a.user_id=h.created_by AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=h.branch_id)) AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'quality.read'))) WHERE h.id=? ON CONFLICT DO NOTHING`).bind(key,id);}
export function prepareQualityScrapNotification(write:SqlDatabase,actor:WorkContext,key:string,countId:string,now:number){return write.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?||'-'||a.user_id,?,a.user_id,'stock_count',?,1,'Quality scrap requires independent Finance review',? ${identityaccessSources.quality_notifications_assignments_a_from} WHERE a.company_id=? AND a.scope_kind='holding' AND iam_assignment_active(a.id,?) AND iam_role_allows(a.role,a.company_id,'inventory.approve') AND NOT iam_same_person(a.user_id,?,?) ON CONFLICT DO NOTHING`).bind(key,actor.companyId,countId,now,actor.companyId,now,actor.userId,actor.companyId);}
export async function routeQualityAlertNotifications(db:SqlDatabase,now:number,scope?:{companyId:string;holdIds:string[]}){await db
    .prepare(
      `INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
       SELECT DISTINCT a.id||':'||m.user_id,h.company_id,m.user_id,'quality_alert',a.id,a.version,
         CASE WHEN a.phase='overdue'
           THEN 'Quality hold is overdue: take responsibility for inspection or release follow-up'
           ELSE 'Quality hold is due within 24 hours: prepare the next inspection or release action'
         END,?::bigint
       ${qualitySources.quality_notifications_inventory_quality_alerts_a_from}
       ${qualitySources.quality_notifications_inventory_quality_holds_h_join} ON h.id=a.hold_id
       ${identityaccessSources.quality_notifications_assignments_m_join} ON ${recipient.replaceAll("clock_timestamp()", "to_timestamp(?::double precision / 1000.0)")}
       WHERE a.acknowledged_by IS NULL AND h.status NOT IN ${terminal}
         AND ${scheduledFeaturePredicate("h.company_id", "quality")}${scope?" AND h.company_id=? AND h.id IN (SELECT jsonb_array_elements_text(?::jsonb))":""}
       ON CONFLICT DO NOTHING`,
    )
    // The recipient expression contains the sweep timestamp once. PostgreSQL
    // still evaluates the rest of the eligibility predicates against current
    // assignment rows, while the testable timestamp makes a replay deterministic.
    .bind(now, now,...(scope?[scope.companyId,JSON.stringify(scope.holdIds)]:[]))
    .run();}
