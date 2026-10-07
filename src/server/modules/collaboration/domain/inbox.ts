import { z } from 'zod';
import type { SourceRecordReference } from '@/lib/source-record';
export type InboxCategory = "work" | "calendar" | "mention" | "notice" | "system";
export type InboxEventInput = {
    key: string;
    companyId: string;
    sourceType: string;
    sourceId: string;
    sourceVersion: number;
    actorUserId?: string | null;
    category: InboxCategory;
    preview: string;
    recipientIds: string[];
    createdAt?: number;
};
export type InboxItem = {
    id: string;
    eventId: string;
    category: InboxCategory;
    message: string;
    href: string | null;
    createdAt: number;
    readAt: number | null;
    archivedAt: number | null;
    sourceType: string;
    sourceId: string;
    source?: SourceRecordReference;
};
export type Candidate = {
    id: string;
    event_id: string;
    company_id: string;
    user_id: string;
    delivered_at: number;
    read_at: number | null;
    archived_at: number | null;
    source_type: string;
    source_id: string;
    source_version: number;
    category: InboxCategory;
    preview: string;
};
export const noticeInput = z.object({
    title: z.string().trim().min(1).max(160), body: z.string().trim().min(1).max(5000),
    recipients: z.array(z.string().min(1).max(160)).max(100).default([]),
    audience: z.object({ kind: z.enum(["company", "branch", "department"]), id: z.string().min(1).max(160) }).optional(),
}).refine(v => Boolean(v.audience) !== (v.recipients.length > 0), "Choose named recipients or one broadcast audience.");
