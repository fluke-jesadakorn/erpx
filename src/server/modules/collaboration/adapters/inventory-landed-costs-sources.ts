import {inventoryReadProjection} from '../../inventory/public/inventory-read-projections';
import {inventoryNotificationAudience} from '../../identity_access/public/inventory-authority-projections';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
export function inventoryLandedCostSources(db:SqlDatabase){return {prepareApproverNotice(value:string,id:string){return db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
 SELECT ?||'-'||a.user_id,c.company_id,a.user_id,'landed_cost',c.id,c.version,'Inventory landed cost needs independent Finance review',c.updated_at
 ${inventoryReadProjection.landedCostNotificationSource} ${inventoryNotificationAudience.landedCostApprovers} ON CONFLICT DO NOTHING`).bind(value,id);},
prepareCreatorNotice(value:string,id:string){return db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
 SELECT ?,company_id,created_by,'landed_cost',id,version,'Inventory landed cost changed: review its status and next action',updated_at
 ${inventoryReadProjection.landedCostCreatorNotificationSource} WHERE id=? ON CONFLICT DO NOTHING`).bind(value,id);}};}
