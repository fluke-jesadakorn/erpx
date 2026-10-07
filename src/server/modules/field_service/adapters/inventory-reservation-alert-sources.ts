import {scheduledFeaturePredicate} from '../../pms/public/hospitality-rollout';
/** Fixed read projection for the owning workflow, consumed by Collaboration's notification writer. */
export function inventoryReservationAlertSelect(phase:'expiring'|'expired',timePredicate:string){return `SELECT 'inventory-reservation-${phase}-'||md5('field:'||r.id||':'||r.expires_at::text||':'||recipient.user_id),o.company_id,recipient.user_id,'field_service_order',o.id,o.version,?,?
      FROM field_service_part_reservations r JOIN field_service_orders o ON o.id=r.order_id
      CROSS JOIN LATERAL(SELECT o.technician_id user_id UNION SELECT o.created_by UNION SELECT r.created_by) recipient
      WHERE r.quantity_milli>r.consumed_milli+r.released_milli AND ${timePredicate} AND o.status IN ('open','scheduled','dispatched','in_visit','exception')
        AND ${scheduledFeaturePredicate("o.company_id", "inventory")}
      ON CONFLICT DO NOTHING`;}
