import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {ClaimedPositionMove} from '@erpx/server-organization/public/position-move-contract';
import type {PositionMoveInput} from "@erpx/feature-people_payroll/lib/position-moves";
export async function appendOrganizationPositionMoveEvents(db:SqlDatabase,row:ClaimedPositionMove,input:PositionMoveInput,affectedCount:number,now:number){
        await db.prepare("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES(?,?,?,?,'organization.position-move.applied','organization_position_move',?,?,?,?)")
          .bind(`audit-${row.id}`, row.id, row.created_by, row.authorizing_assignment_id, row.id, row.version + 1, input.reason, now).run();
        await db.prepare("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES(?,?,'organization.position-move.applied','organization_position_move',?,?,?,?)")
          .bind(`event-${row.id}`, row.id, row.id, row.version + 1, JSON.stringify({ companyId: row.company_id, id: row.id, positionId: row.position_id, affectedCount: affectedCount, state: "applied", effectiveAt: row.effective_at }), now).run();

}
