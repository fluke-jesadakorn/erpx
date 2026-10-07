import { z } from 'zod';
import type { RecordScope } from '../../operations/public/staff-policy';
export const projectSchema = z.object({
    title: z.string().trim().min(2).max(200),
    description: z.string().trim().max(10000).default(""),
    memberUserIds: z.array(z.string().min(1).max(160)).max(200).default([]),
    expectedVersion: z.number().int().positive().default(1),
});
export type ProjectRow = RecordScope & {
    id: string;
    title: string;
    description: string;
    owner_user_id: string;
    lifecycle_status: 'active' | 'handoff_pending' | 'completed' | 'cancelled';
    version: number;
    updated_at: number;
};
