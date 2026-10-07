import {collaborationNotificationSnapshotSources as organizationSnapshotSources} from '../../organization/public/collaboration-notification-snapshot-sources';
import {collaborationNotificationSnapshotSources as identityaccessSnapshotSources} from '../../identity_access/public/collaboration-notification-snapshot-sources';
const organizationSources=organizationSnapshotSources();
const identityaccessSources=identityaccessSnapshotSources();
import "server-only";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
export function rosterNotifications(database: SqlDatabase) {
    return { applyEscalateRosterCoverage(fact0: string, fact1: string, fact2: string, fact3: string, fact4: string, fact5: string, fact6: string, fact7: number, fact8: string, fact9: number, fact10: number, fact11: string | null) {
            return database.prepare(`WITH RECURSIVE ancestors AS (
        SELECT id,parent_id ${organizationSources.people_roster_facts_organization_nodes_from} WHERE id=? AND company_id=?
        UNION ALL SELECT parent.id,parent.parent_id ${organizationSources.people_roster_facts_organization_nodes_parent_from}
          JOIN ancestors child ON child.parent_id=parent.id WHERE parent.company_id=?
      ) INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
      SELECT ? || '-' || a.user_id,?,a.user_id,'roster_coverage',?,1,?,? ${identityaccessSources.people_roster_facts_assignments_a_from}
      WHERE a.company_id=? AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)
        AND iam_role_allows(a.role,a.company_id,'roster.manage')
        AND (a.scope_kind='holding' OR a.scope_kind='branch' AND a.scope_node_id=?
          OR a.scope_kind='department' AND a.scope_node_id IN (SELECT id FROM ancestors))
      ON CONFLICT DO NOTHING`).bind(fact0, fact1, fact2, fact3, fact4, fact5, fact6, fact7, fact8, fact9, fact10, fact11).run();
        },
        applyNotice(fact0: string, fact1: string, fact2: string, fact3: "roster_week" | "roster_request", fact4: string, fact5: number, fact6: string, fact7: number) { return database.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING").bind(fact0, fact1, fact2, fact3, fact4, fact5, fact6, fact7).run(); },
        applyNotifyManagers(fact0: string, fact1: string, fact2: string, fact3: string, fact4: number, fact5: "A roster change needs your review.", fact6: number) { return database.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'roster_request',?,?,?,?) ON CONFLICT DO NOTHING").bind(fact0, fact1, fact2, fact3, fact4, fact5, fact6).run(); } };
}
