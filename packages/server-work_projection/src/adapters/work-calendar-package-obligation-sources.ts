import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** Manual calendar records only. Leave, Roster, lifecycle and other products'
 * minimal busy projections are neither read nor licensed by this contract. */
export const workCalendarSettlementSources={announcements:'SELECT * FROM calendar_announcements',appointments:'SELECT * FROM calendar_appointments',attendees:'SELECT * FROM calendar_attendees',series:'SELECT * FROM calendar_series',exceptions:'SELECT * FROM calendar_occurrence_exceptions',responses:'SELECT * FROM calendar_occurrence_responses',reminders:'SELECT * FROM calendar_reminder_deliveries',history:'SELECT * FROM calendar_history'} as const;
export function lockWorkCalendarSettlementSources(database:SqlDatabase){return database.prepare('LOCK TABLE calendar_announcements,calendar_appointments,calendar_appointment_participants,calendar_attendees,calendar_series,calendar_occurrence_exceptions,calendar_occurrence_responses,calendar_reminder_deliveries,calendar_history IN SHARE MODE').run();}
