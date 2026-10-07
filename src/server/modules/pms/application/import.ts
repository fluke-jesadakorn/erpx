import { z } from "zod";
import { pmsImportSchema, type PmsImportDefinition, type PmsImportReport } from "@/lib/pms-import";
import { executePms, PmsError } from "@/lib/pms-engine";
import { record, records, type Kind, type Snapshot } from "../domain/pms";
import { samePerson } from "@/lib/iam";
import type { WorkContext } from "../../identity_access/public/authorization";
import type { Clock } from "../../shared/application/ports";
export type PmsImportRows<Row> = {
    results: Row[];
};
export type Batch = {
    id: string;
    property_id: string;
    cutover_date: string;
    property_version: number;
    definition_json: string;
    report_json: string;
    state: "preview" | "approved" | "applied";
    created_by: string;
    approved_by: string | null;
    opening_journal_id: string | null;
    version: number;
    created_at: number;
};
export type PmsImportUnit<Effect> = {
    batches(facts: {
        companyId: string;
        propertyId: string;
    }): Promise<PmsImportRows<Batch>>;
    preview(facts: {
        batchId: string;
        companyId: string;
        propertyId: string;
        cutoverDate: string;
        expectedVersion: number;
        fact5: string;
        fact6: string;
        userId: string;
        now: number;
        now2: number;
    }): Effect;
    batch(facts: {
        batchId: string;
        companyId: string;
    }): Promise<Batch | null>;
    customer(facts: {
        partyId: string | undefined;
        companyId: string;
    }): Promise<Record<string, unknown> | null>;
    openingJournal(facts: {
        journalId: string;
        companyId: string;
        chartId: string;
        businessDate: string;
    }): Promise<Record<string, unknown> | null>;
    openingInvoice(facts: {
        openingInvoiceId: string;
        companyId: string;
        journalId: string;
        due: number;
        due2: number;
        receivable: string;
        partyId: string;
    }): Promise<Record<string, unknown> | null>;
    receivableSchedules(facts: {
        companyId: string;
        journalId: string;
    }): Promise<number | null>;
    accountBalance(facts: {
        journalId: string;
        account: string;
    }): Promise<number | null>;
    approve(facts: {
        userId: string;
        openingJournalId: string | null;
        fact2: number;
        id: string;
    }): Effect;
    writeContext(facts: Record<string, never>): Promise<Record<string, unknown> | null>;
    party(facts: {
        partyId: string;
        companyId: string;
        fact2: string;
        name: string;
        now: number;
        now2: number;
    }): Promise<PmsImportRows<Record<string, unknown>>>;
    payment(facts: {
        paymentId: string;
        companyId: string;
        partyId: string;
        fact3: string;
        cutoverDate: string;
        total: number;
        opening_journal_id: string | null;
        created_by: string;
        approved_by: string;
        now: number;
        now2: number;
    }): Promise<PmsImportRows<Record<string, unknown>>>;
    balance(facts: {
        id: string;
        sourceKey: string;
        property_id: string;
        id2: string;
        id3: string;
        pocket: "advance" | "deposit" | "due";
        total: number;
        paymentId: string | null;
        fact8: string | null;
    }): Promise<PmsImportRows<Record<string, unknown>>>;
    document(facts: {
        fact0: string;
        property_id: string;
        id: string;
        fact3: string | null;
        fact4: string;
        now: number;
    }): Promise<PmsImportRows<Record<string, unknown>>>;
    property(facts: {
        now: number;
        property_id: string;
    }): Effect;
    applied(facts: {
        now: number;
        id: string;
    }): Effect;
    snapshot(actor: WorkContext, id: string, lock?: boolean): Promise<Snapshot>;
    persist(snapshot: Snapshot, original: Map<string, string>): Promise<void>;
    replay<Result extends object = Record<string, unknown>>(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Result | null>;
    execute<Result extends object>(actor: WorkContext, key: string, operation: string, input: unknown, id: string, version: number, result: Result, effects: Effect[], scope: {
        companyId: string;
        branchId: string | null;
        departmentId: null;
    }): Promise<Result>;
};
export type PmsImportRepository<Effect> = PmsImportUnit<Effect> & {
    transaction<Result>(work: (unit: PmsImportUnit<Effect>) => Promise<Result>): Promise<Result>;
};
export function createPmsImport<Effect>(ports: {
    clock: Clock;
    hash(value: string): string;
    require(actor: WorkContext, permission: string): void;
    error(status: number, code: string, message: string): Error;
    isBusinessError(error: unknown): error is Error & {
        code: string;
    };
}) {
    const id = (...parts: string[]) => "pms-import-" + ports.hash(JSON.stringify(parts)).slice(0, 40);
    const failed = (code: string, message: string) => ports.error(409, code, message);
    function simulate(initial: Snapshot, definition: PmsImportDefinition, batchId: string, userId: string, now: number) {
        let snapshot = structuredClone(initial);
        const report: PmsImportReport = { errors: [], counts: {}, totals: { due: 0, advance: 0, advanceTax: 0, deposit: 0 }, canApprove: false };
        const maps = new Map<string, Map<string, string>>();
        const links: {
            sourceKey: string;
            folioId: string;
            reservationId: string;
            openingInvoiceId?: string;
        }[] = [];
        for (const category of ["roomTypes", "rooms", "rates", "guests", "reservations"] as const) {
            report.counts[category] = definition[category].length;
            maps.set(category, new Map());
        }
        const resolve = (category: string, key: string) => { const value = maps.get(category)?.get(key); if (!value)
            throw failed("PMS_IMPORT_REFERENCE", `Unknown ${category} key: ${key}`); return value; };
        function command(operation: string, input: unknown, category: string, key: string) {
            let ordinal = 0;
            const result = executePms(snapshot, operation, input, { userId, now, independent: () => true, uuid: () => id(batchId, category, key, String(ordinal++)) });
            if (result.effects.postings.length || result.effects.documents.length)
                throw failed("PMS_IMPORT_POSTING", "Import must not create historical receipts or revenue.");
            return result.id;
        }
        function row(category: string, key: string, run: () => string) {
            const prior = structuredClone(snapshot);
            try {
                if (maps.get(category)!.has(key))
                    throw failed("PMS_IMPORT_DUPLICATE", `Duplicate source key: ${key}`);
                maps.get(category)!.set(key, run());
            }
            catch (error) {
                snapshot = prior;
                if (!(error instanceof PmsError || ports.isBusinessError(error)))
                    throw error;
                report.errors.push({ row: `${category}.${key}`, code: error.code, message: error.message });
            }
        }
        function existing(kind: Kind, value: string | undefined, create: () => string) { if (value) {
            record(snapshot, kind, value);
            return value;
        } return create(); }
        for (const value of definition.roomTypes)
            row("roomTypes", value.key, () => existing("room_type", value.existingId, () => command("room_type.save", value, "roomTypes", value.key)));
        for (const value of definition.rooms)
            row("rooms", value.key, () => existing("room", value.existingId, () => command("room.save", { ...value, typeId: resolve("roomTypes", value.typeKey) }, "rooms", value.key)));
        for (const value of definition.rates)
            row("rates", value.key, () => existing("rate", value.existingId, () => command("rate.save", { ...value, typeId: resolve("roomTypes", value.typeKey), price: value.priceMinor }, "rates", value.key)));
        for (const value of definition.guests)
            row("guests", value.key, () => existing("guest", value.existingId, () => command("guest.save", value, "guests", value.key)));
        for (const value of definition.reservations) {
            for (const pocket of ["due", "advance", "advanceTax", "deposit"] as const)
                report.totals[pocket] += value.opening[pocket];
            row("reservations", value.key, () => {
                if (value.departure <= definition.cutoverDate || value.departure <= value.arrival)
                    throw failed("PMS_IMPORT_DATES", "Import only open stays departing after the cutover date.");
                if (value.state === "in_house" && (!value.roomKey || value.arrival > definition.cutoverDate))
                    throw failed("PMS_IMPORT_ROOM", "An in-house stay needs an assigned room and an arrival on or before cutover.");
                if (value.opening.advanceTax > value.opening.advance)
                    throw failed("PMS_IMPORT_TAX", "Prepaid tax cannot exceed the advance balance.");
                if (value.opening.due && !value.openingInvoiceId)
                    throw failed("PMS_IMPORT_RECEIVABLE", "Opening receivables need the original Finance opening invoice ID.");
                const reservationId = command("reservation.create", { guestId: resolve("guests", value.guestKey), source: definition.source, externalRef: value.key,
                    rooms: [{ typeId: resolve("roomTypes", value.typeKey), rateId: resolve("rates", value.rateKey), arrival: value.arrival < definition.cutoverDate ? definition.cutoverDate : value.arrival, departure: value.departure, adults: value.adults, quantity: 1 }] }, "reservations", value.key);
                const stay = records(snapshot, "stay").find(item => item.data.reservationId === reservationId)!;
                if (value.roomKey)
                    command("stay.assign", { id: stay.id, roomId: resolve("rooms", value.roomKey) }, "assign", value.key);
                // Prices begin at cutover. Earlier room charges belong to the approved opening ledger.
                stay.data.arrival = value.arrival;
                if (value.state === "in_house") {
                    stay.data.state = "in_house";
                    stay.data.checkedInAt = now;
                    for (const segment of stay.data.segments)
                        segment.from = value.arrival;
                }
                const reservation = record(snapshot, "reservation", reservationId);
                reservation.data.history = [{ at: now, actor: userId, action: `Imported at cutover ${definition.cutoverDate}; historical charges remain in opening journal` }];
                const folio = record(snapshot, "folio", stay.data.folioId);
                const importedGuest = definition.guests.find(guest => guest.key === value.guestKey)!;
                if (importedGuest.partyId)
                    folio.data.partyId = importedGuest.partyId;
                Object.assign(folio.data, value.opening);
                for (const pocket of ["due", "advance", "deposit"] as const) {
                    const total = value.opening[pocket];
                    if (!total)
                        continue;
                    snapshot.records.push({ id: id(batchId, value.key, pocket), kind: "entry", version: 1, updatedAt: now, data: { folioId: folio.id, stayId: stay.id, date: definition.cutoverDate,
                            kind: pocket === "due" ? "charge" : pocket === "deposit" ? "deposit" : "payment", description: `Opening ${pocket} · ${definition.source} ${value.key}`,
                            total, net: 0, service: 0, tax: 0, method: "opening", reference: batchId, sourceKey: `opening:${batchId}:${value.key}:${pocket}`, originalId: "", shiftId: "", actorId: userId, at: now } });
                }
                links.push({ sourceKey: value.key, folioId: folio.id, reservationId, openingInvoiceId: value.openingInvoiceId });
                return reservationId;
            });
        }
        report.canApprove = report.errors.length === 0;
        return { snapshot, report, links };
    }
    function checkCutover(snapshot: Snapshot, definition: PmsImportDefinition) {
        if (snapshot.property.version !== definition.expectedVersion)
            throw failed("PMS_IMPORT_STALE", "Hotel records changed. Preview a fresh batch.");
        if (snapshot.property.businessDate !== definition.cutoverDate || snapshot.property.state !== "open")
            throw failed("PMS_IMPORT_CUTOVER", "Use the hotel's open business date as the cutover date.");
        if (snapshot.property.config.currency !== "THB" || !snapshot.property.config.approvedBy)
            throw failed("PMS_IMPORT_ACCOUNTING", "Finance must approve the hotel's THB accounts before importing.");
    }
    async function pmsImportBatches(db: PmsImportUnit<Effect>, actor: WorkContext, propertyId: string) {
        ports.require(actor, "pms.read");
        await db.snapshot(actor, propertyId);
        ports.require(actor, "pms.reports");
        return (await db.batches({ companyId: actor.companyId, propertyId: propertyId })).results.map(batch => ({ ...batch, report: JSON.parse(batch.report_json) as PmsImportReport, report_json: undefined }));
    }
    async function previewPmsImport(db: PmsImportRepository<Effect>, actor: WorkContext, raw: unknown, key: string) {
        ports.require(actor, "pms.configure");
        ports.require(actor, "pms.reservations");
        const definition = pmsImportSchema.parse(raw), batchId = id(actor.companyId, key);
        return db.transaction(async (tx) => {
            const old = await tx.replay(actor, key, "pms.import.preview", batchId, definition);
            if (old)
                return old;
            const snapshot = await tx.snapshot(actor, definition.propertyId, true);
            checkCutover(snapshot, definition);
            const now = ports.clock.now(), { report } = simulate(snapshot, definition, batchId, actor.userId, now), result = { id: batchId, version: 1, state: "preview", report };
            await tx.execute(actor, key, "pms.import.preview", definition, batchId, 1, result, [tx.preview({ batchId: batchId, companyId: actor.companyId, propertyId: definition.propertyId, cutoverDate: definition.cutoverDate, expectedVersion: definition.expectedVersion, fact5: JSON.stringify(definition), fact6: JSON.stringify(report), userId: actor.userId, now: now, now2: now })], { companyId: actor.companyId, branchId: snapshot.property.branchId, departmentId: null });
            return result;
        });
    }
    async function readBatch(db: PmsImportUnit<Effect>, actor: WorkContext, batchId: string) {
        const batch = await db.batch({ batchId: batchId, companyId: actor.companyId });
        if (!batch)
            throw ports.error(404, "PMS_IMPORT_NOT_FOUND", "Import batch not found.");
        const snapshot = await db.snapshot(actor, batch.property_id, true);
        return { batch, snapshot, definition: pmsImportSchema.parse(JSON.parse(batch.definition_json)) };
    }
    async function verifyOpening(db: PmsImportUnit<Effect>, actor: WorkContext, snapshot: Snapshot, report: PmsImportReport, journalId: string | null, definition: PmsImportDefinition, batchId: string) {
        const total = report.totals;
        for (const guest of definition.guests.filter(guest => guest.partyId)) {
            if (!await db.customer({ partyId: guest.partyId, companyId: actor.companyId }))
                throw failed("PMS_IMPORT_CUSTOMER", "Use an active THB Finance customer in this company.");
        }
        if (![total.due, total.advance, total.deposit, total.advanceTax].some(Boolean)) {
            if (journalId)
                throw failed("PMS_IMPORT_JOURNAL", "This batch has no opening balances.");
            return;
        }
        if (!journalId)
            throw failed("PMS_IMPORT_JOURNAL", "Select an independently posted opening journal for the balances.");
        const journal = await db.openingJournal({ journalId: journalId, companyId: actor.companyId, chartId: snapshot.property.config.chartId, businessDate: snapshot.property.businessDate });
        if (!journal)
            throw failed("PMS_IMPORT_JOURNAL", "The journal must be posted, independently approved, in this chart and on the cutover date.");
        const batchReferences = records(snapshot, "entry").filter(entry => entry.data.reference === batchId && entry.data.kind === "charge");
        const invoiceIds = new Set<string>();
        for (const row of definition.reservations.filter(row => row.opening.due > 0)) {
            if (!row.openingInvoiceId || invoiceIds.has(row.openingInvoiceId))
                throw failed("PMS_IMPORT_RECEIVABLE", "Each opening receivable must have a separate original Finance invoice.");
            invoiceIds.add(row.openingInvoiceId);
            const entry = batchReferences.find(entry => entry.id === id(batchId, row.key, "due"))!, folio = record(snapshot, "folio", entry.data.folioId);
            const invoice = await db.openingInvoice({ openingInvoiceId: row.openingInvoiceId, companyId: actor.companyId, journalId: journalId, due: row.opening.due, due2: row.opening.due, receivable: snapshot.property.config.accounts.receivable, partyId: folio.data.partyId });
            if (!invoice)
                throw failed("PMS_IMPORT_RECEIVABLE", "The opening invoice must match the Finance journal, customer, account and guest balance exactly.");
        }
        const schedules = await db.receivableSchedules({ companyId: actor.companyId, journalId: journalId });
        if (Number(schedules) !== invoiceIds.size)
            throw failed("PMS_IMPORT_RECEIVABLE", "Include every receivable schedule from this hotel opening journal exactly once.");
        const accounts = snapshot.property.config.accounts;
        for (const [account, amount] of [[accounts.receivable, total.due], [accounts.advance, -total.advance], [accounts.deposit, -total.deposit], [accounts.prepaidTax, total.advanceTax], [accounts.tax, -total.advanceTax]] as const) {
            const value = await db.accountBalance({ journalId: journalId, account: account });
            if (Number(value) !== amount)
                throw failed("PMS_IMPORT_GL_DIFFERENCE", "Guest opening totals do not match the posted opening control accounts.");
        }
    }
    const pmsImportDecisionSchema = z.object({ id: z.string().min(1).max(160), expectedVersion: z.number().int().positive(), openingJournalId: z.string().min(1).max(160).nullable().default(null) }).strict();
    async function approvePmsImport(db: PmsImportRepository<Effect>, actor: WorkContext, raw: unknown, key: string) {
        ports.require(actor, "pms.accounting.approve");
        ports.require(actor, "finance.read");
        const input = pmsImportDecisionSchema.parse(raw);
        return db.transaction(async (tx) => {
            const old = await tx.replay(actor, key, "pms.import.approve", input.id, input);
            if (old)
                return old;
            const { batch, snapshot, definition } = await readBatch(tx, actor, input.id);
            checkCutover(snapshot, definition);
            if (batch.version !== input.expectedVersion || batch.state !== "preview")
                throw failed("PMS_IMPORT_STALE", "Import batch changed. Refresh it.");
            if (samePerson(actor, batch.created_by))
                throw ports.error(403, "PMS_IMPORT_INDEPENDENT", "Finance approval must be independent of the preparer.");
            const simulated = simulate(snapshot, definition, batch.id, batch.created_by, ports.clock.now()), { report } = simulated;
            if (!report.canApprove)
                throw failed("PMS_IMPORT_ERRORS", "Correct all preview errors before approval.");
            await verifyOpening(tx, actor, simulated.snapshot, report, input.openingJournalId, definition, batch.id);
            const result = { id: batch.id, state: "approved", version: batch.version + 1 };
            await tx.execute(actor, key, "pms.import.approve", input, batch.id, result.version, result, [tx.approve({ userId: actor.userId, openingJournalId: input.openingJournalId, fact2: ports.clock.now(), id: batch.id })], { companyId: actor.companyId, branchId: snapshot.property.branchId, departmentId: null });
            return result;
        });
    }
    async function applyPmsImport(db: PmsImportRepository<Effect>, actor: WorkContext, raw: unknown, key: string) {
        ports.require(actor, "pms.configure");
        ports.require(actor, "pms.reservations");
        const input = z.object({ id: z.string().min(1).max(160), expectedVersion: z.number().int().positive() }).strict().parse(raw);
        return db.transaction(async (tx) => {
            const old = await tx.replay(actor, key, "pms.import.apply", input.id, input);
            if (old)
                return old;
            const { batch, snapshot: initial, definition } = await readBatch(tx, actor, input.id);
            checkCutover(initial, definition);
            if (batch.version !== input.expectedVersion || batch.state !== "approved" || !batch.approved_by)
                throw failed("PMS_IMPORT_APPROVAL", "Finance must approve this unchanged preview before applying it.");
            const now = ports.clock.now(), { snapshot, report, links } = simulate(initial, definition, batch.id, batch.created_by, now);
            if (!report.canApprove)
                throw failed("PMS_IMPORT_ERRORS", "Preview contains errors.");
            await verifyOpening(tx, actor, snapshot, report, batch.opening_journal_id, definition, batch.id);
            await tx.writeContext({});
            for (const entry of records(snapshot, "entry").filter(entry => entry.data.reference === batch.id))
                entry.data.originalId = batch.opening_journal_id ?? "";
            await tx.persist(snapshot, new Map(initial.records.map(item => [item.id, JSON.stringify(item)])));
            for (const link of links) {
                const folio = record(snapshot, "folio", link.folioId), guest = record(snapshot, "guest", folio.data.guestId), partyId = folio.data.partyId || `pms-guest-${guest.id}`;
                await tx.party({ partyId: partyId, companyId: actor.companyId, fact2: `PMS-${guest.id}`, name: guest.data.name, now: now, now2: now });
                for (const entry of records(snapshot, "entry").filter(entry => entry.data.folioId === folio.id && entry.data.reference === batch.id)) {
                    const pocket = entry.data.kind === "charge" ? "due" : entry.data.kind === "deposit" ? "deposit" : "advance", paymentId = pocket === "due" ? null : id(batch.id, link.sourceKey, pocket, "credit");
                    if (paymentId)
                        await tx.payment({ paymentId: paymentId, companyId: actor.companyId, partyId: partyId, fact3: `OPENING-${entry.id}`, cutoverDate: definition.cutoverDate, total: entry.data.total, opening_journal_id: batch.opening_journal_id, created_by: batch.created_by, approved_by: batch.approved_by, now: now, now2: now });
                    await tx.balance({ id: batch.id, sourceKey: link.sourceKey, property_id: batch.property_id, id2: folio.id, id3: entry.id, pocket: pocket, total: entry.data.total, paymentId: paymentId, fact8: pocket === "due" ? link.openingInvoiceId ?? null : null });
                    await tx.document({ fact0: id(entry.id, "document"), property_id: batch.property_id, id: folio.id, fact3: paymentId ?? link.openingInvoiceId ?? null, fact4: JSON.stringify({ property: { name: snapshot.property.name, country: "TH", currency: "THB", branchId: snapshot.property.branchId }, guest: guest.data, folio: folio.data, entries: [entry], amount: entry.data.total, businessDate: definition.cutoverDate, issuedBy: batch.created_by, accountingRevision: snapshot.property.config.revision, openingJournalId: batch.opening_journal_id }), now: now });
                }
            }
            const result = { id: batch.id, state: "applied", version: batch.version + 1, propertyId: batch.property_id, propertyVersion: initial.property.version + 1 };
            await tx.execute(actor, key, "pms.import.apply", input, batch.id, result.version, result, [
                tx.property({ now: now, property_id: batch.property_id }),
                tx.applied({ now: now, id: batch.id })
            ], { companyId: actor.companyId, branchId: snapshot.property.branchId, departmentId: null });
            return result;
        });
    }
    return { pmsImportBatches, previewPmsImport, approvePmsImport, applyPmsImport, pmsImportDecisionSchema };
}
