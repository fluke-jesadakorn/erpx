import type { Clock } from "../../shared/application/ports";
import type { WorkContext } from "../../identity_access/public/authorization";
import type { StaffActor } from "../../operations/public/staff-policy";
import { hasPermission } from "@/lib/permissions";
import { decodePmsSource, pmsSourceAllowed, type PmsSource } from "../domain/source";
export function createPmsSourceApplication(ports: {
    repository: {
        source(actor: StaffActor, id: string): Promise<PmsSource | null>;
        recipients(actor: WorkContext, now: number): Promise<{
            id: string;
            name: string;
        }[]>;
    };
    clock: Clock;
    error(status: number, code: string, message: string): Error;
}) {
    async function readablePmsSource(actor: StaffActor, id: string) { const row = await ports.repository.source(actor, id), decoded = row ? decodePmsSource(row) : null; if (!decoded || !hasPermission(actor, "pms.read") || !pmsSourceAllowed(actor, decoded))
        throw ports.error(404, "PMS_SOURCE_NOT_FOUND", "This hotel work is unavailable."); return decoded; }
    function pmsSourceRecipients(actor: WorkContext) { return ports.repository.recipients(actor, ports.clock.now()); }
    return { readablePmsSource, pmsSourceRecipients };
}
