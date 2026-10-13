import { z } from 'zod';
import { samePerson } from "@/lib/identity_access/iam";
import { accountingProfileSchema, accountingProfileDecisionSchema, financialEventsSchema, type AccountingProfile, type ProfileInput, type ProfileDecision, type FinancialEventsQuery } from '../domain/accounting-profiles';
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import type { FinanceError } from '../domain/invoice-line-revision';
export interface AccountingProfilePorts {
    error: FinanceError;
    authorize(actor: WorkContext, permission: string): WorkContext;
    ready(actor: WorkContext): Promise<void>;
    now(): number;
    replay<Result=Record<string,unknown>>(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Result|null>;
    profiles(facts: {
        actor: WorkContext;
    }): Promise<AccountingProfile[]>;
    activeChart(facts: {
        input: ProfileInput;
        actor: WorkContext;
    }): Promise<{
        base_currency: string;
    } | null>;
    approvedExists(facts: {
        actor: WorkContext;
    }): Promise<boolean>;
    commitPrepareProfile(facts: {
        actor: WorkContext;
        key: string;
        input: ProfileInput;
        id: string;
    }): Promise<CommitPrepareProfileResult>;
    profile(facts: {
        id: string;
        actor: WorkContext;
    }): Promise<AccountingProfile | null>;
    commitProfileDecision(facts: {
        actor: WorkContext;
        key: string;
        operation: string;
        input: ProfileDecision;
        id: string;
        status: "approved" | "rejected";
        decision: "approve" | "reject";
        profile: AccountingProfile;
        now: number;
    }): Promise<CommitProfileDecisionResult>;
    chartExists(facts: {
        query: FinancialEventsQuery;
        actor: WorkContext;
    }): Promise<boolean>;
    events(facts: {
        actor: WorkContext;
        query: FinancialEventsQuery;
    }): Promise<AccountingProfile[]>;
}
export function createAccountingProfiles(ports: AccountingProfilePorts) {
    async function accountingProfiles(actor: WorkContext) {
        actor = ports.authorize(actor, "finance.read");
        await ports.ready(actor);
        return await ports.profiles({ actor });
    }
    async function prepareAccountingProfile(actor: WorkContext, raw: z.input<typeof accountingProfileSchema>, key: string) {
        actor = ports.authorize(actor, "finance.write");
        const input = accountingProfileSchema.parse(raw);
        await ports.ready(actor);
        const id = `accounting-profile-${key}`;
        const previous = await ports.replay<CommitPrepareProfileResult>(actor, key, "finance.profile.prepare", id, input);
        if (previous)
            return previous;
        const chart = await ports.activeChart({ input, actor });
        if (!chart)
            throw ports.error(404, "CHART_NOT_FOUND", "Select an active Company ledger.");
        if (chart.base_currency !== "THB")
            throw ports.error(422, "THAI_STATUTORY_CURRENCY", "A new Thai NPAEs profile requires a THB statutory ledger.");
        if (await ports.approvedExists({ actor }))
            throw ports.error(409, "STATUTORY_PROFILE_EXISTS", "This Company already has an approved statutory ledger. A reviewed migration is required to replace it.");
        return ports.commitPrepareProfile({ actor, key, input, id });
    }
    async function decideAccountingProfile(actor: WorkContext, id: string, decision: "approve" | "reject", raw: z.input<typeof accountingProfileDecisionSchema>, key: string) {
        actor = ports.authorize(ports.authorize(actor, "finance.write"), "finance.approve");
        const input = accountingProfileDecisionSchema.parse(raw);
        await ports.ready(actor);
        const operation = `finance.profile.${decision}`;
        const previous = await ports.replay<CommitProfileDecisionResult>(actor, key, operation, id, input);
        if (previous)
            return previous;
        const profile = await ports.profile({ id, actor });
        if (!profile)
            throw ports.error(404, "PROFILE_NOT_FOUND", "Accounting profile not found in this Company.");
        if (samePerson(actor, profile.prepared_by))
            throw ports.error(403, "INDEPENDENT_APPROVER_REQUIRED", "Another Finance approver must review NPAEs eligibility and Company accounting policies.");
        if (profile.status !== "draft" || profile.version !== input.expectedVersion)
            throw ports.error(409, "VERSION_CONFLICT", "Refresh the profile before reviewing it.");
        const now = ports.now(), status = decision === "approve" ? "approved" : "rejected";
        return ports.commitProfileDecision({ actor, key, operation, input, id, status, decision, profile, now });
    }
    async function financialEvents(actor: WorkContext, query: {
        chartId: string;
        startDate?: string;
        endDate?: string;
    }) {
        actor = ports.authorize(actor, "finance.read");
        query = financialEventsSchema.parse(query);
        await ports.ready(actor);
        if (!await ports.chartExists({ query, actor }))
            throw ports.error(404, "CHART_NOT_FOUND", "Ledger not found in this Company.");
        return await ports.events({ actor, query });
    }
    return { accountingProfiles, prepareAccountingProfile, decideAccountingProfile, financialEvents };
}


export type CommitPrepareProfileResult = { id: string; status: string; version: number; };
export type CommitProfileDecisionResult = { id: string; status: "approved" | "rejected"; version: number; };
