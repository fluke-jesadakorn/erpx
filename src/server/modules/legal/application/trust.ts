import { z } from "zod";
import type { Clock } from "../../shared/application/ports";
import type { LegalWrite } from "./service";
export type LegalTrustMaterial = { id: string; state: string; version: number; created_by: string };
export type LegalTrustAddition = { kind: "root" | "intermediate" | "crl" | "ocsp"; name: string; source: string; content: string };
export type LegalTrustRepository = {
  view(): Promise<Record<string, unknown>[]>; replay(key: string, operation: string, raw: unknown): Promise<Record<string, unknown> | null>;
  lock(): LegalWrite; material(id: string | undefined): Promise<LegalTrustMaterial | null>;
  addition(key: string, input: LegalTrustAddition, base64: string, checksum: string, now: number): LegalWrite[];
  decision(key: string, row: LegalTrustMaterial, state: "approved" | "retired"): LegalWrite[];
  commit(key: string, operation: string, raw: unknown, writes: LegalWrite[]): Promise<{ id: string; version: number }>;
};
export function createLegalTrustApplication(ports: { companyScope: boolean; repository: LegalTrustRepository; require(): void; samePerson(userId: string): boolean; certificate(content: string): { ca: boolean; validTo: string; raw: Uint8Array }; decodeBase64(content: string): Uint8Array; encodeBase64(bytes: Uint8Array): string; hash(bytes: Uint8Array): string; clock: Clock; error(status: number, code: string, message: string): Error }) {
  function fail(message: string, code = "LEGAL_INVALID", status = 422): never { throw ports.error(status, code, message); }
  async function trustView() { ports.require(); return ports.repository.view(); }
  async function trustCommand(raw: unknown, key: string) {
    ports.require(); if (!ports.companyScope) fail("Manage trust at Company scope.");
    const input = z.object({ action: z.enum(["trust.add", "trust.approve", "trust.retire"]), materialId: z.string().optional(), materialVersion: z.number().int().positive().optional() }).parse(raw);
    const prior = await ports.repository.replay(key, input.action, raw); if (prior) return prior;
    const writes = [ports.repository.lock()];
    if (input.action === "trust.add") {
      const addition = z.object({ kind: z.enum(["root", "intermediate", "crl", "ocsp"]), name: z.string().trim().min(3).max(200), source: z.url().refine(value => value.startsWith("https:")), content: z.string().min(20).max(2000000) }).parse(raw);
      let bytes: Uint8Array;
      if (["root", "intermediate"].includes(addition.kind)) {
        const certificate = ports.certificate(addition.content); if (!certificate.ca) fail("A CA certificate is required.");
        if (Date.parse(certificate.validTo) < ports.clock.now()) fail("This CA certificate is expired.");
        bytes = certificate.raw;
      } else { if (!/^[A-Za-z0-9+/=\r\n]+$/.test(addition.content)) fail("Upload base64 DER revocation material."); bytes = ports.decodeBase64(addition.content); }
      writes.push(...ports.repository.addition(key, addition, ports.encodeBase64(bytes), ports.hash(bytes), ports.clock.now()));
    } else {
      const row = await ports.repository.material(input.materialId); if (!row || row.version !== input.materialVersion) return fail("Trust material changed. Refresh.");
      if (input.action === "trust.approve" && (row.state !== "proposed" || ports.samePerson(row.created_by))) fail("A different manager must verify the source and fingerprint.", "LEGAL_INDEPENDENT_REVIEW", 403);
      writes.push(...ports.repository.decision(key, row, input.action === "trust.approve" ? "approved" : "retired"));
    }
    return ports.repository.commit(key, input.action, raw, writes);
  }
  return { trustView, trustCommand };
}
