import { z } from 'zod';
import type {InboxCategory,InboxItem} from '@erpx/contracts/inbox';
export type {InboxCategory,InboxItem} from '@erpx/contracts/inbox';
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
export type Candidate = {
    /** Native read model only; legacy database candidates never supply it. */
    native_field_item?: InboxItem | null;
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
