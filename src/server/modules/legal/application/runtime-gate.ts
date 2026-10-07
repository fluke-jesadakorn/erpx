import type { Clock } from "../../shared/application/ports";
export type LegalCapability = "operations" | "documents" | "ai" | "sources" | "invitations";
export type LegalRuntimeTask = { task: string; heartbeat_at: number; last_success_at: number | null; failure_count: number; last_error: string | null };
export function createLegalRuntimeApplication(ports: { require(): void; tasks(): Promise<LegalRuntimeTask[]>; ready(capability: LegalCapability): Promise<boolean>; clock: Clock }) {
  async function legalRuntimeHealth() {
    ports.require(); const tasks = await ports.tasks();
    return { companyReady: await ports.ready("operations"), tasks: tasks.map(task => ({ ...task, stale: ports.clock.now() - Number(task.heartbeat_at) > 5 * 60_000 })), notice: "Production operation requires current Company release acceptance. Electronic signing and automatic destruction are deferred." };
  }
  return { legalRuntimeHealth };
}
