import { requireCommerceOrderRead } from "@erpx/server-commerce/adapters/purchase-boundary";

import { commerceFinalOwnerProjectionSources as filesProjectionSources } from "@erpx/server-files/public/commerce-final-owner-projection-sources";

import { commercialOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/server-people_payroll/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as salesProjectionSources } from "@erpx/server-sales/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as workflowProjectionSources } from "@erpx/server-workflow/public/commercial-owner-projection-sources";

import { commerceFinanceJoinedFacts } from "@erpx/server-finance/public/commerce-joined-facts";
import { commerceInventoryJoinedFacts } from "@erpx/server-inventory/public/commerce-joined-facts";

import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";

import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { ApiError } from "@erpx/server-runtime/public/http";

import { type Row } from "@erpx/server-commerce/application/commerce";

import { CommerceInventoryFacts } from "@erpx/server-inventory/public/commerce-facts";

import { CommerceFilesFacts } from "@erpx/server-files/public/commerce-facts";

import{createCommerceOrderRead,type CommerceOrderReadUnit}from '@erpx/server-commerce/application/commerce';
export function nativeCommerceOrderReadRepository(db:SqlDatabase):CommerceOrderReadUnit<SqlStatement>{return{readCommerceOrders1: facts => commerceFinanceJoinedFacts(db).readCommerceOrders1(facts),readInventoryActiveReservations2: facts => CommerceInventoryFacts(db).readInventoryActiveReservations2(facts),readCommerceFulfillments1: facts => commerceFinanceJoinedFacts(db).readCommerceFulfillments1(facts),readCommerceReturnLines1: (facts: {
            id: string;
        }) => db.prepare("SELECT fl.*,ol.product_id,ol.item_type,ol.description,p.sku,COALESCE((SELECT SUM(rl.quantity_milli) FROM commerce_return_lines rl JOIN commerce_returns r ON r.id=rl.return_id WHERE rl.fulfillment_line_id=fl.id AND r.status!='rejected'),0) returned_milli FROM commerce_fulfillment_lines fl JOIN commerce_order_lines ol ON ol.id=fl.order_line_id JOIN commerce_products p ON p.id=ol.product_id JOIN commerce_fulfillments f ON f.id=fl.fulfillment_id WHERE f.order_id=? ORDER BY f.created_at,ol.ordinal").bind(facts.id).all<Row>(),readCommerceReturns1: facts => commerceFinanceJoinedFacts(db).readCommerceReturns1(facts),readHrPeople1: (facts: {
            id: string;
        }) => db.prepare(`SELECT a.action,a.actor_id,COALESCE((SELECT legal_name FROM (${people_payrollProjectionSources.hr_people}) p WHERE p.user_id=a.actor_id),a.actor_id) actor_name,a.created_at,a.record_version FROM (${workflowProjectionSources.audit_events}) a WHERE a.record_id=? ORDER BY a.created_at DESC LIMIT 50`).bind(facts.id).all<Row>(),readCommerceOrders4: (facts: {
            id: string;
        }) => db.prepare("SELECT id,number FROM commerce_orders WHERE source_id=?").bind(facts.id).all<Row>(),readInventoryOrderReservations1: facts => CommerceInventoryFacts(db).readInventoryOrderReservations1(facts),readInventoryQualityHolds1: facts => CommerceInventoryFacts(db).readInventoryQualityHolds1(facts),readCommerceReturnLotAllocations1: facts => commerceInventoryJoinedFacts(db).readCommerceReturnLotAllocations1(facts),readCommerceReturnLotAllocations2: facts => commerceInventoryJoinedFacts(db).readCommerceReturnLotAllocations2(facts),readInventoryQualityHolds2: facts => CommerceInventoryFacts(db).readInventoryQualityHolds2(facts),readCommerceFulfillmentEvidence1: (facts: {
            id: string;
        }) => db.prepare(`SELECT e.fulfillment_id,a.id attachment_id,a.original_name,a.media_type,a.bytes,a.checksum_sha256 FROM commerce_fulfillment_evidence e JOIN (${filesProjectionSources.attachments}) a ON a.id=e.attachment_id AND a.revoked_at IS NULL JOIN commerce_fulfillments f ON f.id=e.fulfillment_id WHERE f.order_id=? ORDER BY e.linked_at,e.attachment_id`).bind(facts.id).all<Row>(),readAttachments1: facts => CommerceFilesFacts(db).readAttachments1(facts),readCommerceCustomerNotifications1: (facts: {
            id: string;
        }) => db.prepare(`SELECT * FROM (${salesProjectionSources.commerce_customer_notifications}) commerce_customer_notifications WHERE order_id=? ORDER BY created_at DESC,id`).bind(facts.id).all<Row>(),readCommerceCustomerNotificationSettings1: (facts: {
            companyId: string;
        }) => db.prepare(`SELECT * FROM (${salesProjectionSources.commerce_customer_notification_settings}) commerce_customer_notification_settings WHERE company_id=?`).bind(facts.companyId).first<Row>()};}
const application=createCommerceOrderRead<SqlStatement>({require:requirePermission,error:(status,code,message)=>new ApiError(status,code,message),forActor:async actor=>nativeCommerceOrderReadRepository((await holdingResources(actor.holdingId)).write)});
export async function getOrder(actor: WorkContext, id: string, ownerWrite?: SqlDatabase) {
 const database=ownerWrite??(await holdingResources(actor.holdingId)).write;
 await requireCommerceOrderRead(database,actor,id);
 return application.getOrder(actor,id,ownerWrite?nativeCommerceOrderReadRepository(ownerWrite):undefined);
}
