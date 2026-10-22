import {withTalentPurchasedRead,withTalentHirePurchasedCommand,withTalentPrivacyPurchasedCommand} from "@erpx/server-talent/public/purchases";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { snapshotMoney } from "@erpx/server-finance/public/company-money";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { ApiError } from "@erpx/server-runtime/public/http";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { peopleHireHandoffFacts } from '@erpx/server-talent/public/people-hire-handoff-facts';
import { peopleHirePlacementFacts } from '@erpx/server-organization/public/people-hire-placement-facts';
import { hireConsumptionSchema, candidatePrivacySchema } from '@erpx/server-people_payroll/domain/people-handoffs';
import { createPeopleHandoffApplication, type PeopleHandoffRepository } from '@erpx/server-people_payroll/application/people-handoffs';
export { hireConsumptionSchema, candidatePrivacySchema } from '@erpx/server-people_payroll/domain/people-handoffs';
function application(provided?:SqlDatabase) {
    let database: SqlDatabase;
    const repository: PeopleHandoffRepository<SqlStatement> = { readHireSourceRow(...args) { return peopleHireHandoffFacts(database).readHireSourceRow(...args); },
        readHireHandoffQueueEffect(...args) { return peopleHireHandoffFacts(database).readHireHandoffQueueEffect(...args); },
        readConsumeHireHandoffReceipt(...args) { return peopleHireHandoffFacts(database).readConsumeHireHandoffReceipt(...args); },
        readConsumeHireHandoffGroup(payGroupId: string, companyId: string) { return database.prepare("SELECT id,currency,frequency FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active'").bind(payGroupId, companyId).first<{
            id: string;
            currency: string;
            frequency: string;
        }>(); },
        readConsumeHireHandoffMatches(companyId: string, email: string) { return database.prepare("SELECT id,version,status,merged_into_id FROM hr_people WHERE company_id=? AND lower(email)=lower(?)").bind(companyId, email).all<{
            id: string;
            version: number;
            status: string;
            merged_into_id: string | null;
        }>(); },
        readConsumeHireHandoffPerson(reconcilePersonId: string, companyId: string) { return database.prepare("SELECT id,version,status,merged_into_id FROM hr_people WHERE id=? AND company_id=?").bind(reconcilePersonId, companyId).first<{
            id: string;
            version: number;
            status: string;
            merged_into_id: string | null;
        }>(); },
        stageConsumeHireHandoffEffect(value: string) { return database.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(value); },
        stageConsumeHireHandoffEffect2(...args) { return peopleHireHandoffFacts(database).stageConsumeHireHandoffEffect2(...args); },
        guard9(...args) { return peopleHireHandoffFacts(database).guard9(...args); },
        guard10(...args) { return peopleHireHandoffFacts(database).guard10(...args); },
        guard11(key: string, value: "hire-email-reconcile", companyId: string, email: string, personId: string) { return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM hr_people WHERE company_id=? AND lower(email)=lower(?) AND id<>?)", [companyId, email, personId]); },
        guard12(key: string, value: "hire-group", payGroupId: string, companyId: string, currency: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_pay_groups WHERE id=? AND company_id=? AND currency=? AND status='active')", [payGroupId, companyId, currency]); },
        guard13(...args) { return peopleHirePlacementFacts(database).guard13(...args); },
        guard14(key: string, value: "hire-person", id: string, companyId: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_people WHERE id=? AND company_id=? AND version=? AND status='active' AND merged_into_id IS NULL)", [id, companyId, version]); },
        stageConsumeHireHandoffEffect3(personId: string, companyId: string, legal_name: string, email: string, phone: string | null, countryCode: string, userId: string, now: number, now2: number) { return database.prepare("INSERT INTO hr_people(id,company_id,legal_name,email,phone,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(personId, companyId, legal_name, email, phone, countryCode, userId, now, now2); },
        stageConsumeHireHandoffEffect4(engagementId: string, personId: string, companyId: string, branch_id: string, department_id: string | null, start_date: string, job_title: string, currency: string, payGroupId: string, userId: string, now: number, now2: number) { return database.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,pay_group_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'employee','salaried','onboarding',?,?,?,?,?,?,?)").bind(engagementId, personId, companyId, branch_id, department_id, start_date, job_title, currency, payGroupId, userId, now, now2); },
        stageConsumeHireHandoffEffect5(compensationId: string, engagementId: string, start_date: string, frequency: string, base_amount_minor: number, currency: string, userId: string, now: number, now2: number) { return database.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,currency,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,'draft',?,?,?)").bind(compensationId, engagementId, start_date, frequency, base_amount_minor, currency, userId, now, now2); },
        stageConsumeHireHandoffEffect6(...args) { return peopleHireHandoffFacts(database).stageConsumeHireHandoffEffect6(...args); },
        readChangeCandidatePrivacyRow(...args) { return peopleHireHandoffFacts(database).readChangeCandidatePrivacyRow(...args); },
        stageChangeCandidatePrivacyEffect(...args) { return peopleHireHandoffFacts(database).stageChangeCandidatePrivacyEffect(...args); },
        guard21(...args) { return peopleHireHandoffFacts(database).guard21(...args); },
        stageChangeCandidatePrivacyEffect2(...args) { return peopleHireHandoffFacts(database).stageChangeCandidatePrivacyEffect2(...args); },
        stageChangeCandidatePrivacyEffect3(...args) { return peopleHireHandoffFacts(database).stageChangeCandidatePrivacyEffect3(...args); },
        stageChangeCandidatePrivacyEffect4(...args) { return peopleHireHandoffFacts(database).stageChangeCandidatePrivacyEffect4(...args); } };
    return createPeopleHandoffApplication({ repository, clock: { now: Date.now }, hash: sha256, authorize: requirePermission, prepare: async (holding) => { database = provided??(await holdingResources(holding)).write; }, commands: { replay: (...args) => replayCommand(database, ...args), commit: (...args) => executeCommand(database, ...args) }, money: (actor, input) => snapshotMoney(database, actor, input) });
}
async function invoke<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
export async function hireHandoffQueue(actor: WorkContext) { return withTalentPurchasedRead(actor,["recruit.offers"],db=>invoke(() => application(db).hireHandoffQueue(actor)),undefined,["people.manage"]); }
export async function consumeHireHandoff(actor: WorkContext, input: z.infer<typeof hireConsumptionSchema>, key: string) { return withTalentHirePurchasedCommand(actor,input.handoffId,key,db=>invoke(() => application(db).consumeHireHandoff(actor, input, key))); }
export async function changeCandidatePrivacy(actor: WorkContext, candidateId: string, input: z.infer<typeof candidatePrivacySchema>, key: string) { return withTalentPrivacyPurchasedCommand(actor,candidateId,input,key,db=>invoke(() => application(db).changeCandidatePrivacy(actor, candidateId, input, key))); }
