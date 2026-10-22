import {workSourceReadScope} from "@erpx/server-operations/public/record-purchases";
import {serviceSourceReadScope} from "@erpx/server-field_service/public/record-purchases";
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
import { createInboxApplication, createInboxNoticeApplication, type InboxRepository } from '@erpx/server-collaboration/application/inbox';
import { collaborationInboxSources as owner0 } from '@erpx/server-work_projection/public/collaboration-inbox-sources';
import { collaborationInboxSources as owner1 } from '@erpx/server-identity_access/public/collaboration-inbox-sources';
import { collaborationInboxSources as owner2 } from '@erpx/server-organization/public/collaboration-inbox-sources';
import { collaborationInboxSources as owner3 } from '@erpx/server-workflow/public/collaboration-inbox-sources';
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
const candidateSql = `SELECT d.*,e.source_type,e.source_id,e.source_version,e.category,e.preview
  FROM inbox_deliveries d JOIN inbox_events e ON e.id=d.event_id
  WHERE d.company_id=? AND d.user_id=? AND d.archived_at IS NULL`;
function application(db: SqlDatabase) {
    const repository: InboxRepository = { async publishEvent(id: string, companyId: string, key: string, sourceType: string, sourceId: string, sourceVersion: number, value: string | null, category: InboxCategory, preview: string, now: number) {
            return db.prepare(`INSERT INTO inbox_events(id,company_id,event_key,source_type,source_id,source_version,actor_user_id,category,preview,created_at)
    VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT(company_id,event_key) DO NOTHING`)
                .bind(id, companyId, key, sourceType, sourceId, sourceVersion, value, category, preview, now).run();
        }, async publishedEvent(companyId: string, key: string) {
            return db.prepare("SELECT id FROM inbox_events WHERE company_id=? AND event_key=?")
                .bind(companyId, key).first<{
                id: string;
            }>();
        }, async deliverEvent(value: string, id: string, companyId: string, userId: string, now: number) {
            return db.prepare(`INSERT INTO inbox_deliveries(id,event_id,company_id,user_id,delivered_at)
      VALUES(?,?,?,?,?) ON CONFLICT(event_id,user_id) DO NOTHING`)
                .bind(value, id, companyId, userId, now).run();
        }, async noticeExists(source_id: string, companyId: string) {
            return db.prepare("SELECT id FROM inbox_notices WHERE id=? AND company_id=?")
                .bind(source_id, companyId).first();
        }, async candidate(id: string, companyId: string, userId: string) {
            return db.prepare(`SELECT d.*,e.source_type,e.source_id,e.source_version,e.category,e.preview FROM inbox_deliveries d
    JOIN inbox_events e ON e.id=d.event_id WHERE d.id=? AND d.company_id=? AND d.user_id=?`)
                .bind(id, companyId, userId).first<Candidate>();
        }, async changeDeliveryState(readAt: number | null, archivedAt: number | null, id: string, companyId: string, userId: string) {
            return db.prepare("UPDATE inbox_deliveries SET read_at=?,archived_at=? WHERE id=? AND company_id=? AND user_id=?")
                .bind(readAt, archivedAt, id, companyId, userId).run();
        }, async changeLegacyState(readAt: number | null, id: string, companyId: string, userId: string) {
            return db.prepare("UPDATE staff_notifications SET read_at=? WHERE id=? AND company_id=? AND user_id=?")
                .bind(readAt, id, companyId, userId).run();
        }, async recentNoticeCount(companyId: string, userId: string, value: number) {
            return db.prepare("SELECT count(*)::int n FROM inbox_notices WHERE company_id=? AND sender_user_id=? AND created_at>?")
                .bind(companyId, userId, value).first<{
                n: number;
            }>();
        }, async insertNotice(id: string, companyId: string, userId: string, title: string, body: string, value: string, length: number, now: number) {
            return db.prepare("INSERT INTO inbox_notices(id,company_id,sender_user_id,title,body,audience_json,recipient_count,created_at) VALUES(?,?,?,?,?,?::jsonb,?,?)")
                .bind(id, companyId, userId, title, body, value, length, now).run();
        }, async targetNotice(id: string, companyId: string, userId: string) {
            return db.prepare("INSERT INTO inbox_notice_targets(notice_id,company_id,user_id) VALUES(?,?,?)")
                .bind(id, companyId, userId).run();
        }, async pendingTargets() {
            return db.prepare("SELECT notice_id,company_id,user_id FROM inbox_notice_targets WHERE delivered_at IS NULL ORDER BY notice_id,user_id LIMIT 50")
                .all<{
                notice_id: string;
                company_id: string;
                user_id: string;
            }>();
        }, async noticeEvent(company_id: string, value: string) {
            return db.prepare("SELECT id FROM inbox_events WHERE company_id=? AND event_key=?")
                .bind(company_id, value).first<{
                id: string;
            }>();
        }, async deliverNotice(value: string, id: string, company_id: string, user_id: string, now: number) {
            return db.prepare("INSERT INTO inbox_deliveries(id,event_id,company_id,user_id,delivered_at) VALUES(?,?,?,?,?) ON CONFLICT(event_id,user_id) DO NOTHING")
                .bind(value, id, company_id, user_id, now).run();
        }, async completeNoticeTarget(now: number, notice_id: string, user_id: string) {
            return db.prepare("UPDATE inbox_notice_targets SET delivered_at=? WHERE notice_id=? AND user_id=? AND delivered_at IS NULL")
                .bind(now, notice_id, user_id).run();
        }, async pendingMetrics() {
            return db.prepare(`SELECT count(*)::int count,min(n.created_at) oldest
    FROM inbox_notice_targets t JOIN inbox_notices n ON n.id=t.notice_id WHERE t.delivered_at IS NULL`)
                .first<{
                count: number;
                oldest: number | null;
            }>();
        }, async unresolvedMetrics(types: string[]) {
            return db.prepare(`SELECT count(*)::int count FROM inbox_events WHERE source_type NOT IN (${types.map(() => "?").join(",")})`)
                .bind(...types).first<{
                count: number;
            }>();
        }, ...owner0(db), ...owner1(db), ...owner2(db), ...owner3(db) };
    return createInboxApplication<SqlStatement>({ repository, commands: { replay: (...args) => inward(() => replayCommand(db, ...args)), commit: (...args) => inward(() => executeCommand(db, ...args)) }, clock: { now: () => Date.now() }, identifiers: { next: () => crypto.randomUUID() }, visibleNotifications: (...args) => inward(() => visibleNotifications(db, ...args)), async candidates(actor, view, category, search, cursor) {
            const query = { view },scope=await financeNotificationReadScope(db,actor,"inbox"),basePeople=await peopleSourceReadScope(db,actor,"inbox"),commerce=await commerceSourceReadScope(db,actor,"inbox"),payroll=await payrollSourceReadScope(db,actor,"inbox"),count=await countSourceReadScope(db,actor,"inbox"),procurement=await procurementSourceReadScope(db,actor,"inbox"),transfer=await transferSourceReadScope(db,actor,"inbox"),projects=await projectSourceReadScope(db,actor,"inbox"),quality=await qualitySourceReadScope(db,actor,"inbox"),learning=await learningSourceReadScope(db,actor,"inbox"),assets=await assetSourceReadScope(db,actor,"inbox"),landedCost=await landedCostSourceReadScope(db,actor,"inbox"),manufacturing=await manufacturingSourceReadScope(db,actor,"inbox"),talent=await talentSourceReadScope(db,actor,"inbox"),legal=await legalSourceReadScope(db,actor,"inbox"),crm=await crmSourceReadScope(db,actor,"inbox"),marketing=await marketingSourceReadScope(db,actor,"inbox"),followups=await followupSourceReadScope(db,actor,"inbox"),workforce=await workforceSourceReadScope(db,actor,"inbox"),benefits=await benefitSourceReadScope(db,actor,"inbox"),service=await serviceSourceReadScope(db,actor,"inbox"),work=await workSourceReadScope(db,actor,"inbox");
            return { results: (await db.prepare(`${scope.clock}${candidateSql} AND ${scope.where} AND ${commerce.where} AND ${payroll.where} AND ${basePeople.where} AND ${count.where} AND ${procurement.where} AND ${transfer.where} AND ${projects.where} AND ${quality.where} AND ${learning.where} AND ${assets.where} AND ${landedCost.where} AND ${manufacturing.where} AND ${talent.where} AND ${legal.where} AND ${crm.where} AND ${marketing.where} AND ${followups.where} AND ${workforce.where} AND ${benefits.where} AND ${service.where} AND ${work.where}
      ${query.view === "all" ? "" : "AND d.read_at IS NULL"}
      ${category ? "AND e.category=?" : ""}
      ${search ? "AND e.preview ILIKE '%'||?||'%'" : ""}
      ${cursor ? "AND (d.delivered_at,d.id)<(?,?)" : ""}
      ORDER BY d.delivered_at DESC,d.id DESC LIMIT 100`)
                    .bind(actor.companyId, actor.userId,...scope.args,...commerce.args,...payroll.args,...basePeople.args,...count.args,...procurement.args,...transfer.args,...projects.args,...quality.args,...learning.args,...assets.args,...landedCost.args,...manufacturing.args,...talent.args,...legal.args,...crm.args,...marketing.args,...followups.args,...workforce.args,...benefits.args,...service.args,...work.args, ...(category ? [category] : []), ...(search ? [search] : []), ...(cursor ? [cursor.at, cursor.id] : []))
                    .all<Candidate>()).results };
        } });
}
/** The original candidate resolver runs before this final page projection.
 * Do not perform a destination query from its per-candidate staff resolver. */
async function projectInitialCountInbox(db:SqlDatabase,actor:WorkContext,items:InboxItem[]){
 const ids=[...new Set(items.filter(item=>item.sourceType==='stock_count').map(item=>item.sourceId))];
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
export function inboxSummary(db: SqlDatabase, actor: WorkContext) { return exposed(() => application(db).inboxSummary(actor)); }
export function changeInboxState(db: SqlDatabase, actor: WorkContext, id: string, state: {
    read?: boolean;
    archived?: boolean;
}) { return exposed(() => application(db).changeInboxState(actor, id, state)); }
export function drainInboxNotices(db: SqlDatabase) { return exposed(() => application(db).drainInboxNotices()); }
export function inboxOperationalMetrics(db: SqlDatabase, now = Date.now()) { return exposed(() => application(db).inboxOperationalMetrics(now)); }
export function sendNotice(database: TransactionalSqlDatabase, actor: WorkContext, raw: unknown, key: string) { return exposed(() => createInboxNoticeApplication({ execute: (actor, input, key) => database.transaction(db => application(db).sendPreparedNotice(actor, input, key)) }).sendNotice(actor, raw, key)); }
export { noticeInput } from '@erpx/server-collaboration/domain/inbox';
export type { InboxCategory, InboxEventInput, InboxItem } from '@erpx/server-collaboration/domain/inbox';
