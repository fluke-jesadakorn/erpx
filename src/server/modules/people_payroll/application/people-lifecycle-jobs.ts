import type { LifecycleCase } from '@/lib/people-lifecycle';
import { caseDate, placementEngagementChanged, type LifecycleEngagementFacts, type LifecyclePositionFacts } from '../domain/people-lifecycle-jobs';
export interface PeopleLifecycleWorkerRepository {
    readAvailable(): Promise<{
        rows: ({
            available: boolean;
        })[];
        rowCount: number | null;
    }>;
    readRow(now: number): Promise<{
        rows: (LifecycleCase)[];
        rowCount: number | null;
    }>;
    readPeople(person_id: string, company_id: string): Promise<{
        rows: ({
            status: string;
        })[];
        rowCount: number | null;
    }>;
    readEngagement(engagement_id: string, person_id: string, company_id: string): Promise<{
        rows: (LifecycleEngagementFacts)[];
        rowCount: number | null;
    }>;
    applyEffect(company_id: string, person_id: string, effective_at: number): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    applyEffect2(company_id: string, person_id: string): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    applyEffect3(engagement_id: string, value: "ended" | "suspended", lastWorkingDate: string | null, now: number): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    applyEffect4(company_id: string, person_id: string): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    applyEffect5(id: string, now: number): Promise<{
        rows: ({
            ok: boolean;
        })[];
        rowCount: number | null;
    }>;
    readValid(authorizing_assignment_id: string, now: number): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    readSaved(id: string): Promise<{
        rows: ({
            review_snapshot: unknown;
        })[];
        rowCount: number | null;
    }>;
    readPosition(company_id: string, positionId: string | null): Promise<{
        rows: (LifecyclePositionFacts)[];
        rowCount: number | null;
    }>;
    applyEffect6(manager: string, company_id: string, level_id: unknown): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    applyEffect7(company_id: string, person_id: string, effective_at: number, id: string): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    applyEffect8(company_id: string, person_id: string, effective_at: number, id: string): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    applyEffect9(company_id: string, person_id: string, effective_at: number): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    applyEffect10(company_id: string, person_id: string, effective_at: number): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    applyEffect11(id: string, now: number): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    applyEffect12(id: string): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    applyEffect13(value: string): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    applyEffect14(engagement_id: string, id: unknown, value: unknown, value2: unknown, level_id: unknown, manager: string | null, title: unknown, now: number): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    readPay(company_id: string, id: unknown): Promise<{
        rows: ({
            pay_group_id: string;
            currency: string;
            monthly_amount_minor: number;
        })[];
        rowCount: number | null;
    }>;
    readExisting(engagement_id: string): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    applyEffect15(engagement_id: string, pay_group_id: unknown): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    applyEffect16(value: string, engagement_id: string, value2: string, monthly_amount_minor: unknown, currency: unknown, created_by: string, now: number): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    readPolicy(company_id: string, level_id: unknown): Promise<{
        rows: ({
            policy_version_id: string;
            effective_from: string;
        })[];
        rowCount: number | null;
    }>;
    readPrior(company_id: string, engagement_id: string, date: string): Promise<{
        rows: ({
            id: string;
            effective_from: string;
            effective_to: string | null;
            policy_key: string;
        })[];
        rowCount: number | null;
    }>;
    applyEffect17(until: string, id: unknown): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    applyEffect18(value: string, company_id: string, engagement_id: string, policy_version_id: unknown, effective: unknown, created_by: string, now: number): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    applyEffect19(company_id: string, person_id: string, value: unknown, value2: unknown): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    applyEffect20(value: string, company_id: string, engagement_id: string, value2: string, value3: string, created_by: string, reason: string, id: string, now: number): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    applyEffect21(id: string, now: number): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    applyEffect22(value: string, company_id: string, id: string, value2: string, now: number): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    applyEffect23(value: string, value2: string, id: string, value3: number, value4: string, now: number): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
    applyEffect24(currentId: string, now: number, code: string): Promise<{
        rows: (Record<string, unknown>)[];
        rowCount: number | null;
    }>;
}
export interface PeopleLifecycleWorkerSession {
    repository: PeopleLifecycleWorkerRepository;
    transaction<Result>(run: (session: Pick<PeopleLifecycleWorkerSession, 'repository'>) => Promise<Result>): Promise<Result>;
}
export interface PeopleLifecycleWorkerPorts {
    withSession<Result>(run: (session: PeopleLifecycleWorkerSession) => Promise<Result>): Promise<Result>;
    drainPositionMoves(now: number): Promise<number>;
    drainOrganizationDrafts(now: number): Promise<number>;
}
export function createPeopleLifecycleWorker(ports: PeopleLifecycleWorkerPorts) {
    async function drainPeopleLifecycle(now: number) {
        const processed = await ports.withSession(async (session) => {
            const r = session.repository;
            let processed = 0;
            for (let count = 0; count < 50; count++) {
                let currentId: string | undefined;
                try {
                    const completed = await session.transaction(async (transaction) => {
                        const r = transaction.repository;
                        // A Directory delivery uses the same lock while checking its placement
                        // evidence. Defer this tick without failing the HR case if it is busy.
                        const available = (await r.readAvailable()).rows[0]?.available;
                        if (!available) {
                            return false;
                        }
                        const row = (await r.readRow(now)).rows[0] as LifecycleCase | undefined;
                        if (!row) {
                            return false;
                        }
                        currentId = row.id;
                        const outgoing = row.kind === "offboarding" || row.kind === "suspend";
                        const people = (await r.readPeople(row.person_id, row.company_id)).rows[0];
                        const engagement = (await r.readEngagement(row.engagement_id, row.person_id, row.company_id)).rows[0];
                        if (!people || !engagement)
                            throw Error("LIFECYCLE_REFERENCE_UNAVAILABLE");
                        if (outgoing) {
                            await r.applyEffect(row.company_id, row.person_id, row.effective_at);
                            await r.applyEffect2(row.company_id, row.person_id);
                            await r.applyEffect3(row.engagement_id, row.kind === "offboarding" ? "ended" : "suspended", row.definition.lastWorkingDate, now);
                            await r.applyEffect4(row.company_id, row.person_id);
                        }
                        else {
                            if (row.workflow !== "placement" &&
                                !(await r.applyEffect5(row.id, now)).rows[0].ok)
                                throw Error("LIFECYCLE_REVIEW_CHANGED");
                            let placementSnapshot: Record<string, unknown> | null = null;
                            if (row.workflow === "placement") {
                                const valid = (await r.readValid((row as LifecycleCase & {
                                    authorizing_assignment_id: string;
                                })
                                    .authorizing_assignment_id, now)).rowCount;
                                if (!valid)
                                    throw Error("LIFECYCLE_AUTHORITY_CHANGED");
                                const saved: unknown = (await r.readSaved(row.id)).rows[0]?.review_snapshot;
                                placementSnapshot = saved && typeof saved === "object" && !Array.isArray(saved)
                                    ? saved as Record<string, unknown> : null;
                                if (placementEngagementChanged(engagement, placementSnapshot))
                                    throw Error("LIFECYCLE_PLACEMENT_CHANGED");
                            }
                            const position = (await r.readPosition(row.company_id, row.definition.positionId)).rows[0];
                            if (!position ||
                                engagement.status === "ended" ||
                                people.status !== "active")
                                throw Error("LIFECYCLE_PLACEMENT_CHANGED");
                            if (row.workflow === "placement") {
                                if (position.level_id !== placementSnapshot?.levelId ||
                                    position.branch_id !== placementSnapshot?.positionBranch ||
                                    position.department_id !== placementSnapshot?.positionDepartment)
                                    throw Error("LIFECYCLE_PLACEMENT_CHANGED");
                            }
                            const manager = row.definition.managerPersonId;
                            if (!position.is_ceo &&
                                (!manager ||
                                    !(await r.applyEffect6(manager, row.company_id, position.level_id)).rowCount))
                                throw Error("LIFECYCLE_MANAGER_CHANGED");
                            if (row.workflow !== "placement") {
                                // Only legacy reviewed changes replace previous legacy access. HR placement does not.
                                await r.applyEffect7(row.company_id, row.person_id, row.effective_at, row.id);
                                // Rehire/restoration never revives grants that preceded the cutoff.
                                await r.applyEffect8(row.company_id, row.person_id, row.effective_at, row.id);
                                await r.applyEffect9(row.company_id, row.person_id, row.effective_at);
                                await r.applyEffect10(row.company_id, row.person_id, row.effective_at);
                                await r.applyEffect11(row.id, now);
                            }
                            await r.applyEffect12(row.id);
                            await r.applyEffect13(caseDate(row.effective_at, row.definition.timezone));
                            await r.applyEffect14(row.engagement_id, position.id, position.branch_id ?? engagement.branch_id, position.department_id ?? engagement.department_id, position.level_id, manager, position.title, now);
                            if (row.definition.acceptRecommendedPay) {
                                const pay = (await r.readPay(row.company_id, position.id)).rows[0];
                                const existing = (await r.readExisting(row.engagement_id)).rowCount;
                                if (pay && pay.currency === engagement.currency && !existing) {
                                    await r.applyEffect15(row.engagement_id, pay.pay_group_id);
                                    await r.applyEffect16(`comp-default-lifecycle-${row.id}`, row.engagement_id, caseDate(row.effective_at, row.definition.timezone), pay.monthly_amount_minor, pay.currency, row.created_by, now);
                                }
                            }
                            if (row.definition.acceptRecommendedLeave) {
                                const policy = (await r.readPolicy(row.company_id, position.level_id)).rows[0];
                                if (policy) {
                                    const date = caseDate(row.effective_at, row.definition.timezone);
                                    const prior = (await r.readPrior(row.company_id, row.engagement_id, date)).rows[0];
                                    const managed = !prior || prior.policy_key === "legacy-untracked" || /^level-[1-6]-default$/.test(prior.policy_key);
                                    const changed = Boolean(engagement.level_id && engagement.level_id !== position.level_id);
                                    if (managed && (!prior || changed || prior.policy_key === "legacy-untracked")) {
                                        const effective = changed ? `${Number(date.slice(0, 4)) + 1}-01-01` : date < policy.effective_from ? policy.effective_from : date;
                                        if (prior && prior.effective_from >= effective)
                                            throw Error("LIFECYCLE_LEAVE_ASSIGNMENT_OVERLAP");
                                        const until = new Date(Date.parse(`${effective}T00:00:00Z`) - 86400000).toISOString().slice(0, 10);
                                        if (prior && prior.effective_from <= until && (!prior.effective_to || prior.effective_to > until))
                                            await r.applyEffect17(until, prior.id);
                                        await r.applyEffect18(`leave-default-lifecycle-${row.id}`, row.company_id, row.engagement_id, policy.policy_version_id, effective, row.created_by, now);
                                    }
                                }
                            }
                            await r.applyEffect19(row.company_id, row.person_id, position.branch_id ?? engagement.branch_id, position.department_id ?? engagement.department_id);
                            await r.applyEffect20(`history-${row.id}`, row.company_id, row.engagement_id, JSON.stringify({
                                positionId: engagement.position_id,
                                levelId: engagement.level_id,
                                managerPersonId: engagement.manager_person_id,
                            }), JSON.stringify({
                                positionId: position.id,
                                levelId: position.level_id,
                                managerPersonId: manager,
                            }), row.created_by, row.definition.reason, row.id, now);
                        }
                        await r.applyEffect21(row.id, now);
                        await r.applyEffect22(`applied-${row.id}`, row.company_id, row.id, JSON.stringify({
                            effectiveAt: row.effective_at,
                            kind: row.kind,
                            personId: row.person_id,
                        }), now);
                        await r.applyEffect23(`event-applied-${row.id}`, `system-apply-${row.id}`, row.id, row.version + 1, JSON.stringify({
                            id: row.id,
                            companyId: row.company_id,
                            personId: row.person_id,
                            kind: row.kind,
                            state: "completed",
                            effectiveAt: row.effective_at,
                            version: row.version + 1,
                        }), now);
                        return true;
                    });
                    if (!completed)
                        break;
                    processed++;
                }
                catch (error) {
                    if (!currentId)
                        throw error;
                    // One failed request must not delay other departures. Keep its cutoff active.
                    const code = error instanceof Error && /^LIFECYCLE_[A-Z_]+$/.test(error.message)
                        ? error.message
                        : ((error as {
                            code?: string;
                        }).code ?? "LIFECYCLE_RETRY_REQUIRED");
                    await r.applyEffect24(currentId, now, code);
                }
            }
            return processed;
        });
        return processed + await ports.drainPositionMoves(now) + await ports.drainOrganizationDrafts(now);
    }
    return { drainPeopleLifecycle };
}
