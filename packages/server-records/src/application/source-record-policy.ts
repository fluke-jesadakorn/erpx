import { sourceRecordLabel } from "@erpx/feature-records/lib/source-record";
import { hasModuleAccess } from '@erpx/catalogue/module-access';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { TaskRow } from '@erpx/server-operations/public/staff-tasks-contract';
import type { readablePmsSource, pmsSourceLink } from '@erpx/server-pms/public/pms/source';
import { BusinessRuleError, type Clock } from '@erpx/server-shared/index';
import { sourceRecordInput, sourceOwners, type SourceKind, type SourceRow, type AuthorizedSourceRecord } from '@erpx/server-records/domain/source-record-policy';
import type { z } from 'zod';
export interface SourceRecordPorts<Effect> {
    clock: Clock;
    prepare(holdingId: string): Promise<void>;
    authorize(actor: WorkContext, permission: string): void;
    taskGrant(actor: WorkContext, now: number): Promise<boolean>;
    task(actor: WorkContext, id: string): Promise<TaskRow>;
    pms(actor: WorkContext, id: string): ReturnType<typeof readablePmsSource>;
    pmsLink(source: Awaited<ReturnType<typeof readablePmsSource>>): ReturnType<typeof pmsSourceLink>;
    support(actor: WorkContext, id: string): Promise<Record<string, string | number | null>>;
    source(actor: WorkContext, kind: Exclude<SourceKind, 'staff_task' | 'pms_record' | 'support_ticket'>, id: string): Promise<SourceRow | null>;
    taskGuard(actor: WorkContext, id: string, version: number, key: string, step: string): Effect;
    postGuard(actor: WorkContext, id: string, key: string, step: string): Promise<Effect>;
    pmsGuard(actor: WorkContext, key: string, source: AuthorizedSourceRecord, step: string): Effect;
    supportGuards(actor: WorkContext, key: string, source: AuthorizedSourceRecord, step: string): Promise<Effect[]>;
    sourceGuards(actor: WorkContext, key: string, source: AuthorizedSourceRecord, step: string): Effect[] | Promise<Effect[]>;
}
export function createSourceRecordApplication<Effect>(ports: SourceRecordPorts<Effect>) {
    async function readSourceRecord(a: WorkContext, raw: z.infer<typeof sourceRecordInput>): Promise<AuthorizedSourceRecord> {
        const input = sourceRecordInput.parse(raw);
        await ports.prepare(a.holdingId);
        let row: SourceRow | null = null, href: string;
        if (input.kind === 'staff_task') {
            ports.authorize(a, 'records.read');
            if (!await ports.taskGrant(a, ports.clock.now()))
                throw new BusinessRuleError(404, 'SOURCE_RECORD_NOT_FOUND', 'Source record is unavailable in this work scope.');
            const task = await ports.task(a, input.id);
            row = { id: task.id, company_id: task.company_id, branch_id: task.branch_id, department_id: task.department_id, version: Number(task.version), title: task.title, status: task.state };
            href = `/staff-tasks/${encodeURIComponent(input.id)}`;
        }
        else if (input.kind === 'pms_record') {
            ports.authorize(a, 'pms.read');
            if (a.departmentId)
                throw new BusinessRuleError(404, 'SOURCE_RECORD_NOT_FOUND', 'Source record is unavailable in this work scope.');
            const current = await ports.pms(a, input.id), link = ports.pmsLink(current);
            row = { id: current.id, company_id: current.company_id, branch_id: current.branch_id, department_id: null, version: Number(current.version), title: link.title, status: current.data.state ?? 'open' };
            href = link.href;
        }
        else {
            const owner = sourceOwners[input.kind];
            if (input.kind !== 'support_ticket')
                ports.authorize(a, owner.permission);
            if (['asset', 'field_service_order'].includes(input.kind) && !hasModuleAccess(a.modules, input.kind === 'asset' ? 'assets' : 'field_service'))
                throw new BusinessRuleError(403, 'MODULE_FORBIDDEN', "This source's owning workspace is not available to the selected role.");
            if (['maintenance_plan', 'maintenance_work'].includes(input.kind) && !hasModuleAccess(a.modules, 'maintenance'))
                throw new BusinessRuleError(403, 'MODULE_FORBIDDEN', "This source's owning workspace is not available to the selected role.");
            if (input.kind === 'pos_sale' && a.departmentId)
                throw new BusinessRuleError(404, 'SOURCE_RECORD_NOT_FOUND', 'Source record is unavailable in this work scope.');
            if (input.kind === 'support_ticket') {
                const ticket = await ports.support(a, input.id);
                row = { id: String(ticket.id), company_id: String(ticket.company_id), branch_id: ticket.branch_id ? String(ticket.branch_id) : null, department_id: ticket.department_id ? String(ticket.department_id) : null, version: Number(ticket.version), title: String(ticket.title), status: String(ticket.status) };
            }
            else
                row = await ports.source(a, input.kind, input.id);
            href = `${owner.href}${encodeURIComponent(input.id)}`;
        }
        if (!row)
            throw new BusinessRuleError(404, 'SOURCE_RECORD_NOT_FOUND', 'Source record is unavailable in this work scope.');
        if (input.version !== undefined && input.version !== Number(row.version))
            throw new BusinessRuleError(409, 'SOURCE_RECORD_CHANGED', 'The source version changed. Reload its owning workspace before handing it off.');
        return { kind: input.kind, id: row.id, version: Number(row.version), title: row.title, href, label: `${sourceRecordLabel(input.kind)} · ${row.title}`, companyId: row.company_id, branchId: row.branch_id, departmentId: row.department_id, status: row.status };
    }
    async function visibleSourceRecord(a: WorkContext, input: z.infer<typeof sourceRecordInput>) {
        try {
            return await readSourceRecord(a, input);
        }
        catch (error) {
            if (error instanceof BusinessRuleError && [403, 404].includes(error.status))
                return null;
            throw error;
        }
    }
    async function sourceRecordGuards(a: WorkContext, key: string, source: AuthorizedSourceRecord, step = 'source-record'): Promise<Effect[]> {
        await readSourceRecord(a, { kind: source.kind, id: source.id, version: source.version });
        if (source.kind === 'staff_task') {
            const task = await ports.task(a, source.id), guards = [ports.taskGuard(a, source.id, source.version, key, step)];
            if (task.source_post_id)
                guards.push(await ports.postGuard(a, task.source_post_id, key, `${step}-post`));
            return guards;
        }
        if (source.kind === 'pms_record')
            return [ports.pmsGuard(a, key, source, step)];
        if (source.kind === 'support_ticket')
            return ports.supportGuards(a, key, source, step);
        return ports.sourceGuards(a, key, source, step);
    }
    return { readSourceRecord, visibleSourceRecord, sourceRecordGuards };
}
