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
import { nativeCommerceReturnLineReader } from '@erpx/feature-commerce/public/native-commerce-return-lines';
import { nativeCommerceOrdersConfiguration, nativeCommerceOrdersFailure, type NativeCommerceOrdersConfiguration } from '@erpx/feature-commerce/public/native-commerce-orders';

import { type Row } from "@erpx/server-commerce/application/commerce";
import { nativeCommerceReadAll, RD_ORDER_BY_SOURCE_ID } from './native-commerce-read';

import { CommerceInventoryFacts } from "@erpx/server-inventory/public/commerce-facts";

import { CommerceFilesFacts } from "@erpx/server-files/public/commerce-facts";

import{createCommerceOrderRead,type CommerceOrderReadUnit}from '@erpx/server-commerce/application/commerce';

const returnLineUnavailable = () => new ApiError(
  503,
  'NATIVE_COMMERCE_RETURN_LINE_UNAVAILABLE',
  'The native Commerce return-line reader is unavailable.',
);
const returnLineFailure = (error: unknown): Error => {
  const failure = nativeCommerceOrdersFailure(error);
  if (!failure) return error instanceof Error ? error : returnLineUnavailable();
  return new ApiError(failure.status, failure.code, failure.message);
};
/**
 * Fail-closed bridge to the mounted owner return-line read. The exact Holding
 * target comes from configuration, never from a caller; a missing, ambiguous
 * or malformed target and every failed HTTP read surface an error rather than
 * reaching any TypeScript statement.
 */
async function nativeReturnLines(actor: WorkContext, id: string): Promise<Row[]> {
  const raw = process.env.ERPX_NATIVE_COMMERCE_ORDER_CONFIG;
  if (raw === undefined) throw returnLineUnavailable();
  let config: NativeCommerceOrdersConfiguration;
  let reader: ReturnType<typeof nativeCommerceReturnLineReader>;
  try {
    config = nativeCommerceOrdersConfiguration(raw);
    reader = nativeCommerceReturnLineReader(raw);
  } catch (error) {
    throw returnLineFailure(error);
  }
  const targets = config.holdings.filter(
    (holding) => holding.holdingSlug === actor.holdingSlug && holding.holdingId === actor.holdingId,
  );
  if (targets.length !== 1) throw returnLineUnavailable();
  let host: string;
  try {
    const origin = new URL(targets[0]!.origin);
    if (origin.protocol !== 'https:' || origin.username || origin.password) throw new Error('origin');
    host = origin.host;
  } catch {
    throw returnLineUnavailable();
  }
  try {
    const lines = await reader.read(new Headers({ host }), actor.holdingSlug, actor.companyId, id);
    return lines.map((line) => ({ ...line }));
  } catch (error) {
    throw returnLineFailure(error);
  }
}

export function nativeCommerceOrderReadRepository(db:SqlDatabase):CommerceOrderReadUnit<SqlStatement>{return{readCommerceOrders1: facts => commerceFinanceJoinedFacts(db).readCommerceOrders1(facts),readInventoryActiveReservations2: facts => CommerceInventoryFacts(db).readInventoryActiveReservations2(facts),readCommerceFulfillments1: facts => commerceFinanceJoinedFacts(db).readCommerceFulfillments1(facts),readCommerceReturns1: facts => commerceFinanceJoinedFacts(db).readCommerceReturns1(facts),readHrPeople1: (facts: {
            id: string;
        }) => db.prepare(`SELECT a.action,a.actor_id,COALESCE((SELECT legal_name FROM (${people_payrollProjectionSources.hr_people}) p WHERE p.user_id=a.actor_id),a.actor_id) actor_name,a.created_at,a.record_version FROM (${workflowProjectionSources.audit_events}) a WHERE a.record_id=? ORDER BY a.created_at DESC LIMIT 50`).bind(facts.id).all<Row>(),readCommerceOrders4: (facts: {
            id: string;
        }) => nativeCommerceReadAll<Row>(RD_ORDER_BY_SOURCE_ID,[facts.id],db).then(results=>({results})),readInventoryOrderReservations1: facts => CommerceInventoryFacts(db).readInventoryOrderReservations1(facts),readInventoryQualityHolds1: facts => CommerceInventoryFacts(db).readInventoryQualityHolds1(facts),readCommerceReturnLotAllocations1: facts => commerceInventoryJoinedFacts(db).readCommerceReturnLotAllocations1(facts),readCommerceReturnLotAllocations2: facts => commerceInventoryJoinedFacts(db).readCommerceReturnLotAllocations2(facts),readInventoryQualityHolds2: facts => CommerceInventoryFacts(db).readInventoryQualityHolds2(facts),readCommerceFulfillmentEvidence1: (facts: {
            id: string;
        }) => db.prepare(`SELECT e.fulfillment_id,a.id attachment_id,a.original_name,a.media_type,a.bytes,a.checksum_sha256 FROM commerce_fulfillment_evidence e JOIN (${filesProjectionSources.attachments}) a ON a.id=e.attachment_id AND a.revoked_at IS NULL JOIN commerce_fulfillments f ON f.id=e.fulfillment_id WHERE f.order_id=? ORDER BY e.linked_at,e.attachment_id`).bind(facts.id).all<Row>(),readAttachments1: facts => CommerceFilesFacts(db).readAttachments1(facts),readCommerceCustomerNotifications1: (facts: {
            id: string;
        }) => db.prepare(`SELECT * FROM (${salesProjectionSources.commerce_customer_notifications}) commerce_customer_notifications WHERE order_id=? ORDER BY created_at DESC,id`).bind(facts.id).all<Row>(),readCommerceCustomerNotificationSettings1: (facts: {
            companyId: string;
        }) => db.prepare(`SELECT * FROM (${salesProjectionSources.commerce_customer_notification_settings}) commerce_customer_notification_settings WHERE company_id=?`).bind(facts.companyId).first<Row>()};}
const application=createCommerceOrderRead<SqlStatement>({require:requirePermission,error:(status,code,message)=>new ApiError(status,code,message),forActor:async actor=>nativeCommerceOrderReadRepository((await holdingResources(actor.holdingId)).write),readReturnLines:nativeReturnLines});
export async function getOrder(actor: WorkContext, id: string, ownerWrite?: SqlDatabase) {
 const database=ownerWrite??(await holdingResources(actor.holdingId)).write;
 await requireCommerceOrderRead(database,actor,id);
 return application.getOrder(actor,id,ownerWrite?nativeCommerceOrderReadRepository(ownerWrite):undefined);
}
