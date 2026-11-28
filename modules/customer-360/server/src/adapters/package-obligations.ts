import 'server-only';
import {createHash} from 'node:crypto';
import type {PostgresDatabase} from '@erpx/platform-runtime-server/public/runtime/postgres';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {packageSettlementSource,type PackageSettlementSourceLock} from '@erpx/commercial-server/public/package-settlement-sources';
import {assertPortalAuthorityPackageObligationSources,portalSupportConclusions} from '@erpx/customer-authority-server/public/package-obligations';
import {RD_COMPANY_SCOPE,RD_OBLIGATION_ACTION_HEAD,RD_OBLIGATION_ACK_HEAD,RD_OBLIGATION_AUDIT_EXISTS,nativeC360ReadStatement} from './native-customer-360-read';
const sources=new WeakMap<SqlDatabase,string>();type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};
/** Byte-identical shared support-conclusion existence prefix used by both obligation facts. */
const supportConclusionExists=`EXISTS(SELECT 1 FROM (${portalSupportConclusions}) conclusion WHERE conclusion.`;
export async function lockPortalCustomerPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){const {database,companyId}=packageSettlementSource(owner,lock);assertPortalAuthorityPackageObligationSources(owner,database,companyId);await database.prepare('LOCK TABLE customer_portal_actions IN SHARE ROW EXCLUSIVE MODE').run();sources.set(database,companyId);}
/** Native synchronous staff notifications/outbox are retained evidence, not a
 * claimed external-provider send. Original actions have no external callback. */
export async function portalCustomerPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){owner.assertActiveTransaction(database);if(sources.get(database)!==input.companyId)throw Error('PORTAL_CUSTOMER_PACKAGE_OBLIGATION_SOURCE_REQUIRED');assertPortalAuthorityPackageObligationSources(owner,database,input.companyId);if(input.packageId!=='sales-portal'||input.packageVersion!==1||input.moduleId!=='customer_360'||!Number.isSafeInteger(input.now)||input.now<0)throw Error('PORTAL_CUSTOMER_PACKAGE_OBLIGATION_SCOPE_REQUIRED');
 const scope=await nativeC360ReadStatement(RD_COMPANY_SCOPE,[]);
 const audit=await nativeC360ReadStatement(RD_OBLIGATION_AUDIT_EXISTS,[]);
  const actionHead=await nativeC360ReadStatement(RD_OBLIGATION_ACTION_HEAD,[]);
  const ackHead=await nativeC360ReadStatement(RD_OBLIGATION_ACK_HEAD,[]);
  const facts=(await database.prepare(`WITH scope AS${scope.sql}
 ${actionHead.sql}COALESCE(request.company_id=binding.company_id AND request.order_id=binding.order_id AND request.customer_id=binding.customer_id AND original.company_id=request.company_id AND original.party_id=request.customer_id AND original.kind IN ('quote','sales') AND request.source_version<=original.version AND identity.user_id=request.user_id AND identity.email=binding.email AND ${audit.sql} AND(request.action='acknowledge_delivery' OR request.action='request_support' AND ${supportConclusionExists}request_id=request.id AND conclusion.company_id=request.company_id)),false) pending FROM customer_portal_actions request LEFT JOIN customer_access_bindings binding ON binding.id=request.binding_id LEFT JOIN commerce_orders original ON original.id=request.order_id LEFT JOIN customer_access_identities identity ON identity.binding_id=request.binding_id CROSS JOIN scope WHERE request.company_id=scope.company_id OR binding.company_id=scope.company_id
 UNION ALL ${ackHead.sql}${supportConclusionExists}id=ack.id AND conclusion.company_id=scope.company_id) FROM customer_portal_support_acknowledgements ack CROSS JOIN scope LEFT JOIN customer_access_bindings binding ON binding.id=ack.binding_id WHERE ack.company_id=scope.company_id OR binding.company_id=scope.company_id ORDER BY kind,id`).bind(input.companyId).all<{kind:string;id:string;fact:unknown;pending:boolean|null}>()).results;return {companyId:input.companyId,packageId:input.packageId,moduleId:'customer_360',observedAt:input.now,revision:createHash('sha256').update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts})).digest('hex'),outstanding:facts.filter(row=>row.pending!==false).length};
}
