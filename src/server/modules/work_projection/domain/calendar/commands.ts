import { type RecurrenceRule } from "@/lib/work_projection/calendar-recurrence";
export type Result = {
    id: string;
    version: number;
    status: string;
    warnings: string[];
};
export type SeriesRow = {
    frequency: RecurrenceRule["frequency"];
    interval_count: number;
    weekdays: string;
    end_kind: RecurrenceRule["endKind"];
    occurrence_count: number | null;
    until_date: string | null;
};
