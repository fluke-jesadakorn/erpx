import { cachedHomeRead } from "@erpx/server-work_projection/public/home-cache";
import type {SqlStatement} from '@erpx/server-runtime/public/runtime/types';
import { SelfServicePeoplePayrollOwnedFacts } from '@erpx/server-people_payroll/public/records-self-service-owned-facts';
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { activeStaff } from "@erpx/server-operations/public/staff-policy";
import { leaveSummary } from "@erpx/server-people_payroll/public/leave-self-service";
import {overtimeSetup} from '@erpx/server-people_payroll/public/overtime';
import { workSnapshot } from "@erpx/server-work_projection/public/work-items";
import { holdingResources as nativeHoldingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '@erpx/server-identity_access/public/iam-read-snapshot';

import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
import { createSelfServiceApplication, type SelfServiceRepository } from '@erpx/server-records/application/self-service';
import { BusinessRuleError } from '@erpx/server-shared/index';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: SelfServiceRepository = { readSelfServiceSummarySources(...args) { return SelfServicePeoplePayrollOwnedFacts(database).readSelfServiceSummarySources(...args); } };
    return createSelfServiceApplication<SqlStatement>({repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, prepare: async (holding) => {
            if (!provided)
                database = (await nativeHoldingResources(holding)).write;
        }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, activeStaff: (...args) => fromNative(() => activeStaff(database, ...args)), leaveSummary: (...args) => fromNative(() => leaveSummary(...args)), overtimeSetup: (...args) => fromNative(() => overtimeSetup(database, ...args)), workSnapshot: (...args) => fromNative(() => workSnapshot(...args))});
}
async function fromNative<Result>(run: () => Promise<Result> | Result): Promise<Result> {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof NativeApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
async function invoke<Result>(run: () => Promise<Result>) {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new NativeApiError(error.status, error.code, error.message);
        throw error;
    }
}
export async function selfServiceSummary(actor: WorkContext, onWorkCount?: (count: number | null) => void) {
  const seed = await selfServiceSeed(actor);
  onWorkCount?.(seed.workCount);
  return seed.data;
}
export async function selfServiceSeed(actor: WorkContext): Promise<{
    data: Awaited<ReturnType<ReturnType<typeof application>["selfServiceSummary"]>>;
    workCount: number | null;
}> {
  const { write } = await nativeHoldingResources(actor.holdingId);
  return cachedHomeRead(actor, write, "my-day", null,
    () => invoke(() => application(write).selfServiceSeed(actor)), {
      complete: value => value.workCount !== null && value.data.unavailableSources.length === 0,
      deadline: value => value.data.nextShift?.ends_at ?? Infinity,
    });
}
export type SelfServiceSummary = Awaited<ReturnType<typeof selfServiceSummary>>;
export type SelfServiceSeed = Awaited<ReturnType<typeof selfServiceSeed>>;
