import 'server-only';
import {createHash} from 'node:crypto';
import type {PostgresDatabase} from '@erpx/platform-runtime-server/public/runtime/postgres';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {packageSettlementSource,type PackageSettlementSourceLock} from '@erpx/commercial-server/public/package-settlement-sources';
import {salesCommerceSettlementSources as baseCommerce,salesCommerceSettlementSourceReadLocks} from '@erpx/commerce-server/public/sales-package-sources';
import {crmFinanceSourceReadLocks} from '@erpx/finance-server/public/crm-purchase-sources';
import {ecommerceCommerceSettlementSources} from '@erpx/commerce-server/public/ecommerce-package-sources';
import {scopeTuple} from './read-tail-r29-sql';
import {nativeSalesReadAll,SALES_RD_PACKAGE_SETTLEMENT_SALES,SALES_RD_PACKAGE_SETTLEMENT_ECOMMERCE} from './native-sales-read';
const reconciledAttempt=`EXISTS(SELECT 1 FROM sales_notification_reconciliations resolution JOIN command_receipts receipt ON receipt.command_id=resolution.command_id AND receipt.record_id=notice.id AND receipt.operation='commerce.customer-notification.reconcile' AND receipt.actor_id=resolution.actor_id AND receipt.authority_json::jsonb->>'companyId'=notice.company_id JOIN audit_events audit ON audit.command_id=resolution.command_id AND audit.record_id=notice.id AND audit.record_version=resolution.notification_version AND audit.action=receipt.operation JOIN sales_notification_execution_authorizations authority ON authority.command_id=attempt.authorization_command_id WHERE resolution.attempt_id=attempt.id AND resolution.notification_id=notice.id AND resolution.company_id=notice.company_id AND resolution.source_hash=attempt.source_hash AND resolution.provider_message_id=attempt.provider_message_id AND resolution.provider_message_id=notice.provider_message_id AND resolution.actor_id<>authority.actor_id AND resolution.actor_id<>notice.created_by AND resolution.notification_version=notice.version AND resolution.outcome=notice.status AND receipt.result_json::jsonb->>'reconciliationId'=resolution.id AND receipt.result_json::jsonb->>'status'=resolution.outcome AND receipt.result_json::jsonb->>'version'=resolution.notification_version::text AND resolution.provider_observation_json::jsonb->>'id'=resolution.provider_message_id AND resolution.provider_observation_json::jsonb->'to'=jsonb_build_array(notice.recipient_email) AND resolution.provider_observation_json::jsonb->>'subject'=notice.subject AND resolution.provider_observation_json::jsonb->>'text'=notice.body_text AND resolution.provider_observation_json::jsonb->>'last_event'=resolution.provider_event AND resolution.outcome=CASE WHEN resolution.provider_event IN ('sent','delivered','opened','clicked') THEN 'sent' ELSE 'cancelled' END)`;
/** One parameterized ecommerce order-scope presence family. The alias is an
 * owner-local SQL token, never caller input, so each call renders a
 * byte-identical statement to its original literal. */
const ecommerceOrderScope=(alias:string)=>`EXISTS(SELECT 1 FROM orders o WHERE o.id=${alias}.order_id)`;
/** One parameterized delivery-presence head. The tail is owner-local SQL, never
 * caller input, so each call renders a byte-identical statement to its literal. */
const deliveryPresence=(tail:string)=>`SELECT 1 FROM deliveries f WHERE f.${tail}`;
const orderDeliveryExistsSql=deliveryPresence('order_id=o.id');
const commandAuditPrefixSql=`SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=`;
const allRows=(table:string)=>`SELECT * FROM ${table}`;
const factKind=(kind:string)=>`SELECT '${kind}',`;
/** Owner-local generator for the two scoped settlement-source CTEs. The only
 * varying token is a closed native projection; rendered SQL stays byte-identical. */
const scopedSource=(source:string)=>`MATERIALIZED(SELECT native.* FROM (${source}) native,scope WHERE native.company_id=scope.company_id)`;
/** One parameterized command-receipt presence family. Only fixed operation ids
 * and internally assembled predicates are accepted, so each call renders a
 * byte-identical statement to its original literal. */
const commandReceiptGuard=(operation:string,predicate:string)=>`SELECT 1 FROM command_receipts receipt WHERE receipt.operation='${operation}' AND ${predicate}`;
/** One parameterized notification-attempt presence family. The predicate is
 * owner-local SQL, never caller input, so the rendered statement stays
 * byte-identical to each pre-dedupe literal. */
const attemptGuard=(predicate:string)=>`SELECT 1 FROM sales_notification_execution_attempts attempt WHERE ${predicate}`;
/** One parameterized delivery-fulfillment existence read family. The
 * alias and fixed tail predicate are owner-local tokens; no caller input enters
 * the rendered statement, so each call stays byte-identical to its literal. */
const deliveryFactExists=(alias:string,tail:string)=>deliveryPresence(`id=${alias}.fulfillment_id AND f.order_id=${alias}.order_id${tail}`);
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};const sources=new WeakMap<SqlDatabase,string>();
export async function lockSalesPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){const {database,companyId}=packageSettlementSource(owner,lock);await database.batch([...salesCommerceSettlementSourceReadLocks(database),...crmFinanceSourceReadLocks(database)]);await database.prepare('LOCK TABLE sales_price_lists,sales_price_list_items,customer_followups,customer_followup_events,customer_contact_notes,customer_contact_note_history,commerce_customer_notifications,commerce_customer_notification_settings,sales_notification_execution_authorizations,sales_notification_execution_attempts,sales_notification_reconciliations,command_receipts,audit_events IN SHARE MODE').run();sources.set(database,companyId);}
export function assertSalesPackageObligationSources(owner:PostgresDatabase,database:SqlDatabase,companyId:string){owner.assertActiveTransaction(database);if(sources.get(database)!==companyId)throw Error('SALES_PACKAGE_OBLIGATION_SOURCE_REQUIRED');}
export const salesPackageSettlementSources={...baseCommerce,followups:allRows('customer_followups'),followupEvents:allRows('customer_followup_events'),notices:allRows('commerce_customer_notifications'),noticeAuthorities:allRows('sales_notification_execution_authorizations'),noticeAttempts:allRows('sales_notification_execution_attempts')} as const;
/** Native sources, including failed lineage and provider uncertainty. Shared
 * customer identities and approved historical price books are retained records,
 * rather than an invented perpetual clearing obligation. */
export async function readSalesPackageSettlementFacts(database:SqlDatabase,_companyId:string,selection:'sales'|'ecommerce'){
 const kind=selection==='ecommerce'?SALES_RD_PACKAGE_SETTLEMENT_ECOMMERCE:SALES_RD_PACKAGE_SETTLEMENT_SALES;
 return (await nativeSalesReadAll<{kind:string;id:string;fact:unknown;pending:boolean}>(kind,[_companyId],database)).results;
}
export async function salesPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 assertSalesPackageObligationSources(owner,database,input.companyId);if(input.moduleId!=='sales'||input.packageId!=='sales'||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error('SALES_PACKAGE_OBLIGATION_SCOPE_REQUIRED');
 const facts=await readSalesPackageSettlementFacts(database,input.companyId,'sales');
 return {companyId:input.companyId,packageId:input.packageId,moduleId:'sales',observedAt:input.now,revision:createHash('sha256').update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts})).digest('hex'),outstanding:facts.filter(fact=>fact.pending).length};
}
