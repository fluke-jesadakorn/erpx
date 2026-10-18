import type { CalendarAccess } from "../../domain/calendar/access";
import "server-only";
import type { CalendarQuery } from "@/lib/work_projection/calendar";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { calendarAccess } from "./access";
import { calendarAnnouncementsWorkProjectionFacts } from "../../../work_projection/public/calendar-announcements-facts";
import { createCalendarAnnouncements, type CalendarAnnouncementsUnit } from "../../application/calendar/announcements";
import {calendarPurchaseControls,calendarPurchasedItems} from './purchases';
const service = createCalendarAnnouncements();
export function calendarAnnouncementsUnit(db: SqlDatabase): CalendarAnnouncementsUnit {
    return { read1: facts => calendarAnnouncementsWorkProjectionFacts(db).read1(facts),
        access: actor => calendarAccess(db, actor) };
}
export function announcementAudience(a: CalendarAccess) { return service.announcementAudience(a); }
export async function announcementItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number, publishedOnly = false) {const access=a.manualPurchase??await calendarPurchaseControls(db,a.actor);if(access&&!access.history)return [];return calendarPurchasedItems(await service.announcementItems(calendarAnnouncementsUnit(db), a, q, from, to, limit, publishedOnly),access); }
export async function publishedAnnouncements(db: SqlDatabase, actor: WorkContext, day: string) {const access=await calendarPurchaseControls(db,actor);if(access&&!access.history)return [];return calendarPurchasedItems(await service.publishedAnnouncements(calendarAnnouncementsUnit(db), actor, day),access); }
export type { AnnouncementRow } from "../../domain/calendar/announcements";
