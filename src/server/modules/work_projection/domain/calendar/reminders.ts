import { type RecurrenceRule } from "@/lib/work_projection/calendar-recurrence";
export type Row = {
    id: string;
    company_id: string;
    title: string;
    starts_at: number;
    ends_at: number;
    timezone: string;
    frequency: RecurrenceRule["frequency"] | null;
    interval_count: number | null;
    weekdays: string | null;
    end_kind: RecurrenceRule["endKind"] | null;
    occurrence_count: number | null;
    until_date: string | null;
    user_id: string;
    reminder_minutes: number;
    response: string;
};
