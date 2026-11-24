import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** Manual calendar records only. Leave, Roster, lifecycle and other products'
 * minimal busy projections are neither read nor licensed by this contract. */
const settlementSourceRows=(source:'announcements'|'appointments'|'attendees'|'series'|'exceptions'|'responses'|'reminders'|'history'):string=>{
 const table=source==='announcements'?'calendar_announcements':source==='appointments'?'calendar_appointments':source==='attendees'?'calendar_attendees':source==='series'?'calendar_series':source==='exceptions'?'calendar_occurrence_exceptions':source==='responses'?'calendar_occurrence_responses':source==='reminders'?'calendar_reminder_deliveries':'calendar_history';
 return `SELECT * FROM ${table}`;
};
export const workCalendarSettlementSources={announcements:settlementSourceRows('announcements'),appointments:settlementSourceRows('appointments'),attendees:settlementSourceRows('attendees'),series:settlementSourceRows('series'),exceptions:settlementSourceRows('exceptions'),responses:settlementSourceRows('responses'),reminders:settlementSourceRows('reminders'),history:settlementSourceRows('history')} as const;
export function lockWorkCalendarSettlementSources(database:SqlDatabase){return database.prepare('LOCK TABLE calendar_announcements,calendar_appointments,calendar_appointment_participants,calendar_attendees,calendar_series,calendar_occurrence_exceptions,calendar_occurrence_responses,calendar_reminder_deliveries,calendar_history IN SHARE MODE').run();}
