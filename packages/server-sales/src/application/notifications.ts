import type {NotificationRow} from '@erpx/server-sales/domain/notifications';
/** Retained customer notification settings read. Commands are native-owned. */
export interface CustomerNotificationRepository {settings():Promise<NotificationRow|null>;}
export function createCustomerNotificationApplication(ports:{companyId:string;authorize(permission:'commerce.read'):void;repository():Promise<CustomerNotificationRepository>;}){
 return {
  async settings(){ports.authorize('commerce.read');return await (await ports.repository()).settings()??{company_id:ports.companyId,adapter:'disabled',version:0,updated_by:null,updated_at:null};},
 };
}
