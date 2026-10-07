import {scheduledFeaturePredicate} from '../../pms/public/hospitality-rollout';
/** Fixed read projection for the owning workflow, consumed by Collaboration's notification writer. */
export function inventoryReservationAlertSelect(phase:'expiring'|'expired',timePredicate:string){return `SELECT 'inventory-reservation-${phase}-'||md5('manufacturing:'||r.id||':'||r.expires_at::text||':'||recipient.user_id),o.company_id,recipient.user_id,'manufacturing_order',o.id,o.version,?,?
      FROM manufacturing_material_reservations r JOIN manufacturing_orders o ON o.id=r.order_id
      CROSS JOIN LATERAL(SELECT o.created_by user_id UNION SELECT r.created_by) recipient
      WHERE r.quantity_milli>r.consumed_milli+r.released_milli AND ${timePredicate} AND o.status='reserved'
        AND ${scheduledFeaturePredicate("o.company_id", "inventory")}
      ON CONFLICT DO NOTHING`;}
