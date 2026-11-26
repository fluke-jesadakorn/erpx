import { nativeInboxStatements, nativeInboxSqlFragment, SYSTEM_COMPANY } from './native-inbox-statements';
import {nativeFieldInboxCandidates,nativeFieldInboxState} from './native-field-inbox';
import {workSourceReadScope} from "@erpx/server-operations/public/record-purchases";
import {benefitSourceReadScope} from "@erpx/server-benefits/public/record-purchases";
import {workforceSourceReadScope} from "@erpx/server-people_payroll/public/workforce-record-purchases";
import {followupSourceReadScope} from "@erpx/server-sales/public/followup-purchases";
import {crmSourceReadScope} from "@erpx/server-crm/public/record-purchases";
import {marketingSourceReadScope} from "@erpx/server-marketing/public/record-purchases";
import {legalSourceReadScope} from "@erpx/server-legal/public/record-purchases";
import {talentSourceReadScope} from "@erpx/server-talent/public/record-purchases";
import {manufacturingSourceReadScope} from "@erpx/server-manufacturing/public/record-purchases";
import {landedCostSourceReadScope} from "@erpx/server-inventory/public/landed-cost-record-purchases";
import {assetSourceReadScope} from "@erpx/server-assets/public/record-purchases";
import {learningSourceReadScope} from "@erpx/server-learning/public/record-purchases";
import {qualitySourceReadScope} from "@erpx/server-quality/public/record-purchases";
import {projectSourceReadScope} from "@erpx/server-projects/public/record-purchases";
import {transferSourceReadScope} from "@erpx/server-inventory/public/transfer-record-purchases";
import {procurementSourceReadScope} from "@erpx/server-procurement/public/record-purchases";
import {countSourceReadScope,initialPhysicalStockCountDestinations} from "@erpx/server-inventory/public/count-record-purchases";
import {peopleSourceReadScope,payrollSourceReadScope} from "@erpx/server-people_payroll/public/record-purchases";
import {commerceSourceReadScope} from "@erpx/server-commerce/public/record-purchases";
import {financeNotificationReadScope} from "@erpx/server-finance/public/document-purchases";
import 'server-only';
import type { SqlDatabase, SqlStatement, TransactionalSqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { ApiError } from '@erpx/server-runtime/public/http';
import { BusinessRuleError } from '@erpx/server-shared/index';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { InboxCategory, InboxEventInput, Candidate, InboxItem } from '@erpx/server-collaboration/domain/inbox';
import { executeCommand, replayCommand } from '@erpx/server-workflow/public/commands';
import { visibleNotifications } from '@erpx/server-collaboration/public/staff-notifications';
import { nativeInboxSummarySSR } from '@erpx/server-collaboration/public/native-inbox-page';
import { createInboxApplication, createInboxNoticeApplication, type InboxFailedScope, type InboxRepository } from '@erpx/server-collaboration/application/inbox';
import { collaborationInboxSources as owner0 } from '@erpx/server-work_projection/public/collaboration-inbox-sources';
import { collaborationInboxSources as owner1 } from '@erpx/server-identity_access/public/collaboration-inbox-sources';
import { collaborationInboxSources as owner2 } from '@erpx/server-organization/public/collaboration-inbox-sources';
import { collaborationInboxSources as owner3 } from '@erpx/server-workflow/public/collaboration-inbox-sources';
import {legacyNativeUnion} from './read-tail-sql';
async function inward<T>(action: () => Promise<T>) {
    try {
        return await action();
    }
    catch (error) {
        if (error instanceof ApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
async function exposed<T>(action: () => Promise<T>) {
    try {
        return await action();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
function application(db: SqlDatabase, failedScope?: InboxFailedScope) {
    const repository: InboxRepository = { async publishEvent(id: string, companyId: string, key: string, sourceType: string, sourceId: string, sourceVersion: number, value: string | null, category: InboxCategory, preview: string, now: number) {
            return await nativeInboxStatements(db, companyId).eventInsert([id, companyId, key, sourceType, sourceId, sourceVersion, value, category, preview, now]);
        }, async publishedEvent(companyId: string, key: string) {
            return (await nativeInboxStatements(db, companyId).eventByKey(companyId, key)).first<{
                id: string;
            }>();
        }, async deliverEvent(value: string, id: string, companyId: string, userId: string, now: number) {
            return await nativeInboxStatements(db, companyId).deliveryInsert([value, id, companyId, userId, now]);
        }, async noticeExists(source_id: string, companyId: string) {
            return (await nativeInboxStatements(db, companyId).noticeExists(companyId, source_id)).first();
        }, async candidate(id: string, companyId: string, userId: string) {
            return (await nativeInboxStatements(db, companyId).candidate(companyId, id, userId)).first<Candidate>();
        }, async changeDeliveryState(readAt: number | null, archivedAt: number | null, id: string, companyId: string, userId: string) {
            return await nativeInboxStatements(db, companyId).deliveryState([readAt, archivedAt, id, companyId, userId]);
        }, async changeLegacyState(readAt: number | null, id: string, companyId: string, userId: string) {
            return await nativeInboxStatements(db, companyId).staffNotificationRead([readAt, id, companyId, userId]);
        }, async recentNoticeCount(companyId: string, userId: string, value: number) {
            return (await nativeInboxStatements(db, companyId).recentNoticeCount(companyId, userId, value)).first<{
                n: number;
            }>();
        }, async insertNotice(id: string, companyId: string, userId: string, title: string, body: string, value: string, length: number, now: number) {
            return await nativeInboxStatements(db, companyId).noticeInsert([id, companyId, userId, title, body, value, length, now]);
        }, async targetNotice(id: string, companyId: string, userId: string) {
            return await nativeInboxStatements(db, companyId).noticeTargetInsert([id, companyId, userId]);
        }, async pendingTargets() {
            return (await nativeInboxStatements(db, SYSTEM_COMPANY).pendingTargets()).all<{
                notice_id: string;
                company_id: string;
                user_id: string;
            }>();
        }, async noticeEvent(company_id: string, value: string) {
            return (await nativeInboxStatements(db, company_id).eventByKey(company_id, value)).first<{
                id: string;
            }>();
        }, async deliverNotice(value: string, id: string, company_id: string, user_id: string, now: number) {
            return await nativeInboxStatements(db, company_id).deliveryInsert([value, id, company_id, user_id, now]);
        }, async completeNoticeTarget(now: number, notice_id: string, user_id: string, companyId: string) {
            return await nativeInboxStatements(db, companyId).noticeTargetComplete([now, notice_id, user_id]);
        }, async pendingMetrics() {
            return (await nativeInboxStatements(db, SYSTEM_COMPANY).pendingMetrics()).first<{
                count: number;
                oldest: number | null;
            }>();
        }, async unresolvedMetrics(types: string[]) {
            return (await nativeInboxStatements(db, SYSTEM_COMPANY).unresolvedMetrics(types)).first<{
                count: number;
            }>();
        }, ...owner0(db), ...owner1(db), ...owner2(db), ...owner3(db, failedScope
            ? { holdingId: failedScope.holdingId, holdingSlug: failedScope.holdingSlug }
            : undefined) };
    return createInboxApplication<SqlStatement>({ repository, commands: { replay: (...args) => inward(() => replayCommand(db, ...args)), commit: (...args) => inward(() => executeCommand(db, ...args)) }, clock: { now: () => Date.now() }, identifiers: { next: () => crypto.randomUUID() }, visibleNotifications: (...args) => inward(() => visibleNotifications(db, ...args)), async candidates(actor, view, category, search, cursor) {
            const native=await nativeFieldInboxCandidates(actor,{all:view==='all',...(category?{category}:{}),search,...(cursor?{before:cursor}:{}),limit:100});
            // Keep the validated native projection outside the SQL adapter, whose JSONB
            // compatibility contract returns JSON text. PostgreSQL owns ordering only.
            const nativeItems=new Map(native.map(row=>[row.id,row.native_field_item]));
            const query = { view },scope=await financeNotificationReadScope(db,actor,"inbox"),basePeople=await peopleSourceReadScope(db,actor,"inbox"),commerce=await commerceSourceReadScope(db,actor,"inbox"),payroll=await payrollSourceReadScope(db,actor,"inbox"),count=await countSourceReadScope(db,actor,"inbox"),procurement=await procurementSourceReadScope(db,actor,"inbox"),transfer=await transferSourceReadScope(db,actor,"inbox"),projects=await projectSourceReadScope(db,actor,"inbox"),quality=await qualitySourceReadScope(db,actor,"inbox"),learning=await learningSourceReadScope(db,actor,"inbox"),assets=await assetSourceReadScope(db,actor,"inbox"),landedCost=await landedCostSourceReadScope(db,actor,"inbox"),manufacturing=await manufacturingSourceReadScope(db,actor,"inbox"),talent=await talentSourceReadScope(db,actor,"inbox"),legal=await legalSourceReadScope(db,actor,"inbox"),crm=await crmSourceReadScope(db,actor,"inbox"),marketing=await marketingSourceReadScope(db,actor,"inbox"),followups=await followupSourceReadScope(db,actor,"inbox"),workforce=await workforceSourceReadScope(db,actor,"inbox"),benefits=await benefitSourceReadScope(db,actor,"inbox"),work=await workSourceReadScope(db,actor,"inbox");
            const [nativeRows,candidateSource,combinedOrder]=await Promise.all([
                nativeInboxSqlFragment(actor.companyId,'inbox_native_rows_cte',[native]),
                nativeInboxSqlFragment(actor.companyId,'inbox_candidate_source'),
                nativeInboxSqlFragment(actor.companyId,'inbox_combined_order'),
            ]);
            return { results: (await db.prepare(`${nativeRows}${scope.clock}${candidateSource} AND NOT (e.source_type IN('field_service_order','operational_exception','collaboration_action','support_ticket','stock_count','invoice','journal') AND erpx_field_push_source(e.source_type,e.source_id,e.company_id))  AND ${scope.where} AND ${commerce.where} AND ${payroll.where} AND ${basePeople.where} AND ${count.where} AND ${procurement.where} AND ${transfer.where} AND ${projects.where} AND ${quality.where} AND ${learning.where} AND ${assets.where} AND ${landedCost.where} AND ${manufacturing.where} AND ${talent.where} AND ${legal.where} AND ${crm.where} AND ${marketing.where} AND ${followups.where} AND ${workforce.where} AND ${benefits.where} AND ${work.where}
      ${query.view === "all" ? "" : "AND d.read_at IS NULL"}
      ${category ? "AND e.category=?" : ""}
      ${search ? "AND e.preview ILIKE '%'||?||'%'" : ""}
      ${cursor ? "AND (d.delivered_at,d.id)<(?,?)" : ""}
      ORDER BY d.delivered_at DESC,d.id DESC LIMIT 100), combined AS ${legacyNativeUnion} ${combinedOrder}`)
                    .bind(JSON.stringify(native),actor.companyId, actor.userId,...scope.args,...commerce.args,...payroll.args,...basePeople.args,...count.args,...procurement.args,...transfer.args,...projects.args,...quality.args,...learning.args,...assets.args,...landedCost.args,...manufacturing.args,...talent.args,...legal.args,...crm.args,...marketing.args,...followups.args,...workforce.args,...benefits.args,...work.args, ...(category ? [category] : []), ...(search ? [search] : []), ...(cursor ? [cursor.at, cursor.id] : []))
                    .all<Candidate>()).results.map(row=>nativeItems.has(row.id)?{...row,native_field_item:nativeItems.get(row.id)}:row) };
        } });
}
/** The original candidate resolver runs before this final page projection.
 * Do not perform a destination query from its per-candidate staff resolver. */
async function projectInitialCountInbox(db:SqlDatabase,actor:WorkContext,items:InboxItem[]){
 const ids=[...new Set(items.filter(item=>item.sourceType==='stock_count'&&item.href!==`/services/field-service/settings/initial-stock/${encodeURIComponent(item.sourceId)}`).map(item=>item.sourceId))];
 if(!ids.length)return items;
 const destinations=await initialPhysicalStockCountDestinations(db,actor,ids);
 return items.map(item=>{const href=item.sourceType==='stock_count'&&Object.hasOwn(destinations,item.sourceId)?destinations[item.sourceId]:undefined;return href?{...item,href,...(item.source?{source:{...item.source,href}}:{})}:item;});
}
export function publishInboxEvent(db: SqlDatabase, input: InboxEventInput) { return exposed(() => application(db).publishInboxEvent(input)); }
export function inboxPage(db: SqlDatabase, actor: WorkContext, query: {
    view?: string;
    category?: string;
    search?: string;
    cursor?: string;
    limit?: number;
}) { return exposed(async () => {const page=await application(db).inboxPage(actor,query);return {...page,items:await projectInitialCountInbox(db,actor,page.items)};}); }
/** Native-only summary read. The owner /inbox/summary projection replaces the
 * TS page-loop aggregation; there is no TS read fallback. Missing native
 * transport fails closed (503) inside the owner transport. */
export function inboxSummary(_db: SqlDatabase, actor: WorkContext) { return exposed(() => nativeInboxSummarySSR(actor.holdingSlug)); }
async function nativeInboxDelivery(db:SqlDatabase,actor:WorkContext,id:string){
    const row=await (await nativeInboxStatements(db,actor.companyId).deliveryNativeOwned(actor.companyId,id,actor.userId)).first<{source_type:string;native_owned:number}>();
        return row?.native_owned===1;
}
export function changeInboxState(db: SqlDatabase, actor: WorkContext, id: string, state: {read?:boolean;archived?:boolean},key?:string) {
    return exposed(async()=>{
        if(await nativeInboxDelivery(db,actor,id))return nativeFieldInboxState(actor,id,JSON.stringify(state),key??'');
        return application(db).changeInboxState(actor,id,state);
    });
}
/** The old Staff URL is a transport consumer of the same native command. Other
 * owner notices keep their existing scalar read acknowledgement semantics. */
export function markStaffNotificationRead(db:SqlDatabase,actor:WorkContext,id:string,key:string,now=Date.now()){
    return exposed(async()=>{
        if(await nativeInboxDelivery(db,actor,id))return nativeFieldInboxState(actor,id,'{"read":true}',key);
        return await nativeInboxStatements(db, actor.companyId).staffNotificationRead([now,id,actor.companyId,actor.userId]);
    });
}
export function drainInboxNotices(db: SqlDatabase) { return exposed(() => application(db).drainInboxNotices()); }
export function inboxOperationalMetrics(db: SqlDatabase, failedScope?: InboxFailedScope, now = Date.now()) {
    return exposed(() => application(db, failedScope).inboxOperationalMetrics(now, failedScope));
}
export function sendNotice(database: TransactionalSqlDatabase, actor: WorkContext, raw: unknown, key: string) { return exposed(() => createInboxNoticeApplication({ execute: (actor, input, key) => database.transaction(db => application(db).sendPreparedNotice(actor, input, key)) }).sendNotice(actor, raw, key)); }
export { noticeInput } from '@erpx/server-collaboration/domain/inbox';
export type { InboxCategory, InboxEventInput, InboxItem } from '@erpx/server-collaboration/domain/inbox';
