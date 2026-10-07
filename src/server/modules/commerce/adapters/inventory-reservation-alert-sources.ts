import { commercialOwnerProjectionSources as inventoryProjectionSources } from "../../inventory/public/commercial-owner-projection-sources";
import { scheduledFeaturePredicate } from '../../pms/public/hospitality-rollout';
/** Fixed read projection for the owning workflow, consumed by Collaboration's notification writer. */
export function inventoryReservationAlertSelect(phase: 'expiring' | 'expired', timePredicate: string) {
    return `SELECT 'inventory-reservation-${phase}-'||md5('sales:'||r.id||':'||r.expires_at::text||':'||recipient.user_id),o.company_id,recipient.user_id,'commerce_order',o.id,o.version,?,?
      FROM (${inventoryProjectionSources.inventory_order_reservations}) r JOIN commerce_order_lines l ON l.id=r.order_line_id JOIN commerce_orders o ON o.id=l.order_id
      CROSS JOIN LATERAL(SELECT o.created_by user_id UNION SELECT r.created_by) recipient
      WHERE r.quantity_milli>r.consumed_milli+r.released_milli AND ${timePredicate} AND o.state IN ('confirmed','partially_fulfilled')
        AND ${scheduledFeaturePredicate("o.company_id", "inventory")}
      ON CONFLICT DO NOTHING`;
}
