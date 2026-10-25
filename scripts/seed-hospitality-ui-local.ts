/** Add fictional, unpaid hotel examples without changing existing guests or bookings. */
import { readFile, writeFile, mkdir, rename } from "node:fs/promises";
import { createHash } from "node:crypto";
import { dirname, resolve } from "node:path";
import { mock } from "bun:test";
import { assertHospitalityDevelopmentTarget } from "../src/lib/hospitality-seed-target";
import { plusDays, records } from "../src/lib/pms-model";
import type { WorkContext } from "../src/server/authorization";
import { PostgresDatabase } from "../src/server/runtime/postgres";
import { isTransactionalSqlDatabase } from "../src/server/runtime/types";

mock.module("next/navigation", () => ({ forbidden: () => { throw new Error("CLI_PAGE_FORBIDDEN"); }, redirect: () => { throw new Error("CLI_REDIRECT_FORBIDDEN"); } }));
const path = process.env.ERPX_DATABASE_MANIFEST;
if (!path) throw new Error("ERPX_DATABASE_MANIFEST_REQUIRED");
const manifest = JSON.parse(await readFile(path, "utf8"));
const jobsPath = resolve(dirname(path), "databases-jobs.json");
const jobs = JSON.parse(await readFile(jobsPath, "utf8"));
for (const target of [manifest, jobs]) assertHospitalityDevelopmentTarget(target, process.env.ERPX_ENVIRONMENT);
process.env.ERPX_DATABASE_MANIFEST = jobsPath;
process.env.ERPX_PROCESS_ROLE = "jobs";
const { getRuntime, closeRuntime } = await import("../src/server/runtime/env");
const { loadPms, pmsCommand } = await import("../src/server/pms");
const env = getRuntime();
const db = env.H001_DB;
if (!(db instanceof PostgresDatabase)) throw new Error("POSTGRES_RUNTIME_REQUIRED");
if (!isTransactionalSqlDatabase(db)) throw new Error("POSTGRES_TRANSACTION_REQUIRED");
const guide = JSON.parse(await readFile(".scratch/dev/hospitality-acme/scenario-guide.json", "utf8"));
if (guide.companyId !== "acme-co" || guide.branchId !== "bangkok") throw new Error("ACME_BASELINE_REQUIRED");
const actor: WorkContext = {
  userId: "acme-hospitality-v1-administrator", assignmentId: "acme-hospitality-v1-administrator-access", role: "holding_owner",
  companyId: "acme-co", branchId: null, departmentId: null, holdingId: "h001", holdingSlug: "acme", holdingName: "ACME",
  name: "Fictional Acme administrator", email: "administrator@acme-hospitality-v1.example", assignmentScope: "holding", inherited: false,
  modules: ["pms", "pos", "finance", "inventory", "commerce", "organization", "iam", "people"], assignments: [], workspaces: [],
  scope: { kind: "holding", holdingId: "h001", holdingSlug: "acme", holdingName: "ACME", companyId: "acme-co" },
};
const digest = createHash("sha256").update(new URL(jobs.databases.H001_DB).pathname).digest("hex").slice(0, 12);
const directory = ".scratch/dev/hospitality-ui";
const statePath = `${directory}/state-${digest}.json`;
type Step = { body: Parameters<typeof pmsCommand>[2]; result?: { id: string } };
type State = { businessDate: string; steps: Record<string, Step> };
let state: State;
async function persist() {
  await mkdir(directory, { recursive: true, mode: 0o700 });
  await writeFile(`${statePath}.tmp`, JSON.stringify(state, null, 2), { mode: 0o600 });
  await rename(`${statePath}.tmp`, statePath);
}
try {
  const snapshot = await loadPms(db, actor, guide.propertyId);
  state = await readFile(statePath, "utf8").then(JSON.parse).catch(error => {
    if (error.code !== "ENOENT") throw error;
    return { businessDate: snapshot.property.businessDate, steps: {} };
  });
  const names = ["อรุณ · ผู้เข้าพักตัวอย่าง", "Maya · Example guest", "ลลิน · ผู้เข้าพักตัวอย่าง", "Noah · Example guest"];
  const plan = names.map((name, i) => ({ name, arrival: plusDays(state.businessDate, i < 2 ? 0 : i - 1), departure: plusDays(state.businessDate, i < 2 ? 2 : i + 1), roomNumber: ["101", "201", "102", "202"][i] }));
  const summary = { developmentOnly: true, company: "acme-co", propertyId: guide.propertyId, businessDate: state.businessDate, reservations: plan, housekeeping: ["103", "204"], completedSteps: Object.values(state.steps).filter(step => step.result).length, policy: "Seed-owned examples only; no payments, check-ins, income posting, permissions or existing-record edits." };
  if (process.argv.includes("--verify")) {
    const expected = Object.values(state.steps).map(step => step.result?.id).filter(Boolean);
    const present = new Set(snapshot.records.map(row => row.id));
    const missing = expected.filter(id => !present.has(id!));
    const reservationIds = new Set(plan.map((_, i) => state.steps[`reservation:${i}`]?.result?.id).filter(Boolean));
    const stays = records(snapshot, "stay").filter(row => reservationIds.has(row.data.reservationId));
    const guestAccounts = new Set(stays.map(row => row.data.guestAccountId));
    const entries = records(snapshot, "entry").filter(row => guestAccounts.has(row.data.guestAccountId));
    const proof = { company: "acme-co", propertyId: guide.propertyId, propertyVersion: snapshot.property.version, completedSteps: expected.length, reservations: reservationIds.size, stays: stays.length, assignedStays: stays.filter(row => row.data.roomId).length, entries: entries.length, missing };
    console.log(JSON.stringify(proof, null, 2));
    if (expected.length !== 14 || missing.length || reservationIds.size !== 4 || stays.length !== 4) throw new Error("ACME_UI_SEED_INCOMPLETE");
  } else if (!process.argv.includes("--apply")) console.log(JSON.stringify(summary, null, 2));
  else {
    const command = async (label: string, operation: string, input: unknown) => {
      if (state.steps[label]?.result) return state.steps[label].result!;
      if (!state.steps[label]) {
        const current = await loadPms(db, actor, guide.propertyId);
        state.steps[label] = { body: { operation, propertyId: guide.propertyId, expectedVersion: current.property.version, input } };
        await persist();
      }
      const key = `acme-ui-${digest}-${createHash("sha256").update(label).digest("hex").slice(0, 24)}`;
      const result = await pmsCommand(db, actor, state.steps[label].body, key);
      state.steps[label].result = { id: result.id };
      await persist();
      return state.steps[label].result!;
    };
    for (const [i, item] of plan.entries()) {
      const current = await loadPms(db, actor, guide.propertyId);
      const room = records(current, "room").find(row => row.data.number === item.roomNumber);
      if (!room) throw new Error("ACME_SEED_ROOM_REQUIRED");
      const rate = records(current, "rate").find(row => row.data.typeId === room.data.typeId && String(row.data.name).startsWith("Acme rehearsal"));
      if (!rate) throw new Error("ACME_SEED_RATE_REQUIRED");
      const guest = await command(`guest:${i}`, "guest.save", { name: item.name, email: `guest${i + 1}@acme-ui.example`, phone: "", notes: "Fictional UX rehearsal; no real identity or contact data." });
      const reservation = await command(`reservation:${i}`, "reservation.create", { guestId: guest.id, source: "Acme UI rehearsal", externalRef: `ACME-UI-${digest}-${i}`, notes: "Fictional unpaid example. No income or receipt posted.", rooms: [{ typeId: room.data.typeId, rateId: rate.id, arrival: item.arrival, departure: item.departure, adults: 2, childAges: [], quantity: 1 }] });
      const stay = records(await loadPms(db, actor, guide.propertyId), "stay").find(row => row.data.reservationId === reservation.id);
      if (!stay) throw new Error("ACME_SEED_STAY_REQUIRED");
      await command(`assign:${i}`, "stay.assign", { id: stay.id, roomId: room.id, reason: "Assign fictional UI example to a seed-owned room" });
    }
    for (const number of ["103", "204"]) {
      const room = records(await loadPms(db, actor, guide.propertyId), "room").find(row => row.data.number === number);
      if (!room) throw new Error("ACME_SEED_ROOM_REQUIRED");
      await command(`task:${number}`, "task.create", { roomId: room.id, type: "arrival", date: state.businessDate });
    }
    console.log(JSON.stringify({ ...summary, applied: true, completedSteps: Object.values(state.steps).filter(step => step.result).length }, null, 2));
  }
} finally { await closeRuntime(); }
