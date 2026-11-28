import type { CalendarAccess } from "@erpx/work-projection-server/domain/calendar/access";
import "server-only";
import type { CalendarQuery } from "@erpx/work-projection-ui/lib/calendar";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { calendarAccess } from "@erpx/work-projection-server/adapters/calendar/access";
import { calendarAnnouncementsWorkProjectionFacts } from "@erpx/work-projection-server/public/calendar-announcements-facts";
import { createCalendarAnnouncements, type CalendarAnnouncementsUnit } from "@erpx/work-projection-server/application/calendar/announcements";
import {calendarPurchaseControls,calendarPurchasedItems} from '@erpx/work-projection-server/adapters/calendar/purchases';
import { bindLane29Context } from '../native-lane-29/client';
const service = createCalendarAnnouncements();
export function calendarAnnouncementsUnit(db: SqlDatabase, actor?: WorkContext): CalendarAnnouncementsUnit {
    if (actor) bindLane29Context(db, actor);
    return { read1: facts => calendarAnnouncementsWorkProjectionFacts(db).read1(facts),
        access: actor => calendarAccess(db, actor) };
}
export function announcementAudience(a: CalendarAccess) { return service.announcementAudience(a); }
export async function announcementItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number, publishedOnly = false) {const access=a.manualPurchase??await calendarPurchaseControls(db,a.actor);if(access&&!access.history)return [];return calendarPurchasedItems(await service.announcementItems(calendarAnnouncementsUnit(db, a.actor), a, q, from, to, limit, publishedOnly),access); }
export async function publishedAnnouncements(db: SqlDatabase, actor: WorkContext, day: string) {const access=await calendarPurchaseControls(db,actor);if(access&&!access.history)return [];return calendarPurchasedItems(await service.publishedAnnouncements(calendarAnnouncementsUnit(db, actor), actor, day),access); }
export type { AnnouncementRow } from "@erpx/work-projection-server/domain/calendar/announcements";
