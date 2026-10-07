import type { SourceRecordReference } from "@/lib/source-record";
import { sourceRecordKind, sourceRecordInput } from '../../shared';
import type { z } from 'zod';
export { sourceRecordKind, sourceRecordInput };
export type SourceKind = z.infer<typeof sourceRecordKind>;
export type SourceRow = {
    id: string;
    company_id: string;
    branch_id: string | null;
    department_id: string | null;
    version: number;
    title: string;
    status: string;
};
export type AuthorizedSourceRecord = SourceRecordReference & {
    kind: SourceKind;
    version: number;
    title: string;
    companyId: string;
    branchId: string | null;
    departmentId: string | null;
    status: string;
};
export const sourceOwners = { commerce_order: { permission: 'commerce.read', href: '/commerce/orders/' }, support_ticket: { permission: 'support.read', href: '/support/' }, field_service_order: { permission: 'records.read', href: '/field-service/' }, pos_sale: { permission: 'pos.read', href: '/pos?sale=' }, asset: { permission: 'records.read', href: '/assets/' }, maintenance_plan: { permission: 'records.read', href: '/maintenance/plans/' }, maintenance_work: { permission: 'records.read', href: '/maintenance/' } } as const;
