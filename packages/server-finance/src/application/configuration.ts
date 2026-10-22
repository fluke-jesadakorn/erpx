import { z } from 'zod';
import { createChartSchema, createAccountSchema, changeAccountStatusSchema, createPeriodSchema, partySchema, taxCodeSchema, budgetSchema, leaseSchema, loanSchema, revenueContractSchema, revenueObligationSchema } from '@erpx/server-finance/domain/finance';
import { createFinancePolicy } from '@erpx/server-finance/domain/finance-policy';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { FinanceError } from '@erpx/server-finance/domain/invoice-line-revision';
export interface FinanceConfigurationPorts {
    error: FinanceError;
    authorize(actor: WorkContext, permission: string): WorkContext;
    now(): number;
    id(): string;
    ready(actor: WorkContext): Promise<void>;
    prior<Result=Record<string,unknown>>(actor: WorkContext, id: string, operation: string, input: unknown): Promise<Result|null>;
    moneyScale(companyId: string, currency: string): Promise<number>;
    commitCreateChart(facts: {
        chartId: string;
        actor: WorkContext;
        input: z.infer<typeof createChartSchema>;
        now: number;
        id: string;
        result: CommitCreateChartResult;
    }): Promise<CommitCreateChartResult>;
    accountChart(facts: {
        input: z.infer<typeof createAccountSchema>;
        actor: WorkContext;
    }): Promise<{
        id: string;
        base_currency: string;
    } | null>;
    commitCreateAccount(facts: {
        accountId: string;
        input: z.infer<typeof createAccountSchema>;
        actor: WorkContext;
        chart: {
            id: string;
            base_currency: string;
        };
        now: number;
        id: string;
        result: CommitCreateAccountResult;
    }): Promise<CommitCreateAccountResult>;
    statusAccount(facts: {
        accountId: string;
        chartId: string;
        actor: WorkContext;
    }): Promise<{
        id: string;
        status: string;
        version: number;
        is_control: number;
    } | null>;
    draftAccountUsage(facts: {
        accountId: string;
        actor: WorkContext;
    }): Promise<boolean>;
    openAccountUsage(facts: {
        actor: WorkContext;
        accountId: string;
    }): Promise<boolean>;
    commitChangeAccountStatus(facts: {
        actor: WorkContext;
        id: string;
        operation: string;
        chartId: string;
        accountId: string;
        input: z.infer<typeof changeAccountStatusSchema>;
        result: CommitChangeAccountStatusResult;
        account: {
            id: string;
            status: string;
            version: number;
            is_control: number;
        };
        now: number;
    }): Promise<CommitChangeAccountStatusResult>;
    commitCreatePeriod(facts: {
        periodId: string;
        actor: WorkContext;
        input: z.infer<typeof createPeriodSchema>;
        now: number;
        id: string;
        result: CommitCreatePeriodResult;
    }): Promise<CommitCreatePeriodResult>;
    commitCreateParty(facts: {
        partyId: string;
        actor: WorkContext;
        input: z.infer<typeof partySchema>;
        now: number;
        id: string;
        output: CommitCreatePartyResult;
    }): Promise<CommitCreatePartyResult>;
    commitCreateTaxCode(facts: {
        taxId: string;
        actor: WorkContext;
        input: z.infer<typeof taxCodeSchema>;
        rate: number;
        recoverable: number;
        id: string;
        output: CommitCreateTaxCodeResult;
    }): Promise<CommitCreateTaxCodeResult>;
    commitCreateBudget(facts: {
        budgetId: string;
        actor: WorkContext;
        input: z.infer<typeof budgetSchema>;
        now: number;
        id: string;
        output: CommitCreateBudgetResult;
    }): Promise<CommitCreateBudgetResult>;
    commitCreateLease(facts: {
        leaseId: string;
        actor: WorkContext;
        input: z.infer<typeof leaseSchema>;
        payment: number;
        rate: number;
        now: number;
        id: string;
        output: CommitCreateLeaseResult;
    }): Promise<CommitCreateLeaseResult>;
    commitCreateLoan(facts: {
        loanId: string;
        actor: WorkContext;
        input: z.infer<typeof loanSchema>;
        principal: number;
        now: number;
        id: string;
        output: CommitCreateLoanResult;
    }): Promise<CommitCreateLoanResult>;
    revenueCustomer(facts: {
        input: z.infer<typeof revenueContractSchema>;
        actor: WorkContext;
    }): Promise<boolean>;
    commitCreateRevenueContract(facts: {
        contractId: string;
        actor: WorkContext;
        input: z.infer<typeof revenueContractSchema>;
        now: number;
        id: string;
        output: CommitCreateRevenueContractResult;
    }): Promise<CommitCreateRevenueContractResult>;
    revenueContract(facts: {
        input: z.infer<typeof revenueObligationSchema>;
        actor: WorkContext;
    }): Promise<boolean>;
    commitCreateRevenueObligation(facts: {
        obligationId: string;
        input: z.infer<typeof revenueObligationSchema>;
        actor: WorkContext;
        id: string;
        output: CommitCreateRevenueObligationResult;
    }): Promise<CommitCreateRevenueObligationResult>;
}
export function createFinanceConfiguration(ports: FinanceConfigurationPorts) {
    const { decimalToMinor } = createFinancePolicy(ports.error);
    async function createChart(actor: WorkContext, input: z.infer<typeof createChartSchema>, id: string) {
        actor = ports.authorize(actor, "finance.write");
        await ports.ready(actor);
        const previous = await ports.prior<CommitCreateChartResult>(actor, id, "finance.chart.create", input);
        if (previous)
            return previous;
        const chartId = `chart-${ports.id()}`;
        const now = ports.now();
        const result = { id: chartId, ...input, companyId: actor.companyId, version: 1 };
        return ports.commitCreateChart({ chartId, actor, input, now, id, result });
    }
    async function createAccount(actor: WorkContext, input: z.infer<typeof createAccountSchema>, id: string) {
        actor = ports.authorize(actor, "finance.write");
        await ports.ready(actor);
        const previous = await ports.prior<CommitCreateAccountResult>(actor, id, "finance.account.create", input);
        if (previous)
            return previous;
        const chart = await ports.accountChart({ input, actor });
        if (!chart)
            throw ports.error(404, "CHART_NOT_FOUND", "Chart not found.");
        const accountId = `acct-${ports.id()}`;
        const now = ports.now();
        const result = { id: accountId, ...input, companyId: actor.companyId, currency: input.currency ?? chart.base_currency, version: 1 };
        return ports.commitCreateAccount({ accountId, input, actor, chart, now, id, result });
    }
    async function changeAccountStatus(actor: WorkContext, chartId: string, accountId: string, input: z.infer<typeof changeAccountStatusSchema>, id: string) {
        actor = ports.authorize(actor, "finance.write");
        await ports.ready(actor);
        const operation = `finance.account.${input.status}`;
        const previous = await ports.prior<CommitChangeAccountStatusResult>(actor, id, operation, { chartId, accountId, ...input });
        if (previous)
            return previous;
        const account = await ports.statusAccount({ accountId, chartId, actor });
        if (!account)
            throw ports.error(404, "ACCOUNT_NOT_FOUND", "The account is not available in this chart.");
        if (Number(account.version) !== input.expectedVersion)
            throw ports.error(409, "VERSION_CONFLICT", "The account changed. Refresh before changing its status.");
        if (account.status === input.status)
            throw ports.error(409, "ACCOUNT_STATUS_UNCHANGED", `The account is already ${input.status}.`);
        if (input.status === "inactive") {
            const draft = await ports.draftAccountUsage({ accountId, actor });
            if (draft)
                throw ports.error(409, "ACCOUNT_HAS_DRAFT_JOURNAL", "Post or correct draft journals before making this account inactive.");
            const outstanding = await ports.openAccountUsage({ actor, accountId });
            if (outstanding)
                throw ports.error(409, "ACCOUNT_HAS_OPEN_INVOICES", "Resolve open invoices before making this account inactive.");
        }
        const now = ports.now(), result = { id: accountId, chartId, status: input.status, version: input.expectedVersion + 1 };
        return ports.commitChangeAccountStatus({ actor, id, operation, chartId, accountId, input, result, account, now });
    }
    async function createPeriod(actor: WorkContext, input: z.infer<typeof createPeriodSchema>, id: string) {
        actor = ports.authorize(actor, "finance.write");
        await ports.ready(actor);
        const previous = await ports.prior<CommitCreatePeriodResult>(actor, id, "finance.period.create", input);
        if (previous)
            return previous;
        if (input.endDate < input.startDate)
            throw ports.error(422, "INVALID_PERIOD", "Period end must be on or after its start.");
        const periodId = `period-${ports.id()}`;
        const now = ports.now();
        const result = { id: periodId, ...input, state: "open", companyId: actor.companyId, version: 0 };
        return ports.commitCreatePeriod({ periodId, actor, input, now, id, result });
    }
    async function createParty(actor: WorkContext, input: z.infer<typeof partySchema>, id: string) {
        actor = ports.authorize(actor, "finance.write");
        await ports.ready(actor);
        const previous = await ports.prior<CommitCreatePartyResult>(actor, id, "finance.party.create", input);
        if (previous)
            return previous;
        await ports.moneyScale(actor.companyId, input.currency);
        const partyId = `party-${ports.id()}`;
        const now = ports.now();
        const output = { id: partyId, ...input, creditLimitMinor: decimalToMinor(input.creditLimit) };
        return ports.commitCreateParty({ partyId, actor, input, now, id, output });
    }
    async function createTaxCode(actor: WorkContext, input: z.infer<typeof taxCodeSchema>, id: string) {
        actor = ports.authorize(actor, "finance.write");
        await ports.ready(actor);
        const previous = await ports.prior<CommitCreateTaxCodeResult>(actor, id, "finance.tax-code.create", input);
        if (previous)
            return previous;
        const taxId = `tax-${ports.id()}`;
        const rate = decimalToMinor(input.rate, 6);
        const recoverable = decimalToMinor(input.recoverable, 6);
        if (rate < 0 || recoverable < 0 || recoverable > 1000000)
            throw ports.error(422, "INVALID_TAX_RATE", "Tax rates are non-negative and recoverability is between 0 and 1.");
        const output = { id: taxId, ...input, rateMicros: rate, recoverableMicros: recoverable };
        return ports.commitCreateTaxCode({ taxId, actor, input, rate, recoverable, id, output });
    }
    async function createBudget(actor: WorkContext, input: z.infer<typeof budgetSchema>, id: string) {
        actor = ports.authorize(actor, "finance.write");
        await ports.ready(actor);
        const previous = await ports.prior<CommitCreateBudgetResult>(actor, id, "finance.budget.create", input);
        if (previous)
            return previous;
        const budgetId = `budget-${ports.id()}`;
        const now = ports.now();
        const output = { id: budgetId, ...input, status: "draft" };
        return ports.commitCreateBudget({ budgetId, actor, input, now, id, output });
    }
    async function createLease(actor: WorkContext, input: z.infer<typeof leaseSchema>, id: string) {
        actor = ports.authorize(actor, "finance.write");
        await ports.ready(actor);
        const previous = await ports.prior<CommitCreateLeaseResult>(actor, id, "finance.lease.create", input);
        if (previous)
            return previous;
        await ports.moneyScale(actor.companyId, input.currency);
        const leaseId = `lease-${ports.id()}`;
        const now = ports.now();
        const payment = decimalToMinor(input.payment);
        const rate = decimalToMinor(input.discountRate, 6);
        const output = { id: leaseId, status: "draft", payment: input.payment };
        return ports.commitCreateLease({ leaseId, actor, input, payment, rate, now, id, output });
    }
    async function createLoan(actor: WorkContext, input: z.infer<typeof loanSchema>, id: string) {
        actor = ports.authorize(actor, "finance.write");
        await ports.ready(actor);
        const previous = await ports.prior<CommitCreateLoanResult>(actor, id, "finance.loan.create", input);
        if (previous)
            return previous;
        await ports.moneyScale(actor.companyId, input.currency);
        const loanId = `loan-${ports.id()}`;
        const now = ports.now();
        const principal = decimalToMinor(input.principal);
        const output = { id: loanId, status: "draft", principal: input.principal };
        return ports.commitCreateLoan({ loanId, actor, input, principal, now, id, output });
    }
    async function createRevenueContract(actor: WorkContext, input: z.infer<typeof revenueContractSchema>, id: string) {
        actor = ports.authorize(actor, "finance.write");
        await ports.ready(actor);
        const previous = await ports.prior<CommitCreateRevenueContractResult>(actor, id, "finance.revenue-contract.create", input);
        if (previous)
            return previous;
        await ports.moneyScale(actor.companyId, input.currency);
        const customer = await ports.revenueCustomer({ input, actor });
        if (!customer)
            throw ports.error(404, "CUSTOMER_NOT_FOUND", "Customer not found.");
        const contractId = `contract-${ports.id()}`;
        const now = ports.now();
        const output = { id: contractId, status: "draft", transactionPrice: input.transactionPrice };
        return ports.commitCreateRevenueContract({ contractId, actor, input, now, id, output });
    }
    async function createRevenueObligation(actor: WorkContext, input: z.infer<typeof revenueObligationSchema>, id: string) {
        actor = ports.authorize(actor, "finance.write");
        await ports.ready(actor);
        const previous = await ports.prior<CommitCreateRevenueObligationResult>(actor, id, "finance.revenue-obligation.create", input);
        if (previous)
            return previous;
        const contract = await ports.revenueContract({ input, actor });
        if (!contract)
            throw ports.error(404, "CONTRACT_NOT_FOUND", "Revenue contract not found.");
        const obligationId = `obligation-${ports.id()}`;
        const output = { id: obligationId, status: "pending", allocated: input.allocated };
        return ports.commitCreateRevenueObligation({ obligationId, input, actor, id, output });
    }
    return { createChart, createAccount, changeAccountStatus, createPeriod, createParty, createTaxCode, createBudget, createLease, createLoan, createRevenueContract, createRevenueObligation };
}


export type CommitCreateChartResult = { companyId: string; version: number; name: string; baseCurrency: string; accountingBasis: string; id: string; };
export type CommitCreateAccountResult = { companyId: string; currency: string; version: number; chartId: string; code: string; name: string; category: "asset" | "liability" | "equity" | "revenue" | "expense" | "other_income" | "other_expense"; isControl: boolean; controlKind: "none" | "receivable" | "payable" | "bank"; parentId?: string | null | undefined; id: string; };
export type CommitChangeAccountStatusResult = { id: string; chartId: string; status: "active" | "inactive"; version: number; };
export type CommitCreatePeriodResult = { state: string; companyId: string; version: number; name: string; startDate: string; endDate: string; id: string; };
export type CommitCreatePartyResult = { creditLimitMinor: number; kind: "customer" | "supplier" | "employee"; code: string; name: string; currency: string; paymentTermsDays: number; creditLimit: string; taxRegistration?: string | undefined; id: string; };
export type CommitCreateTaxCodeResult = { rateMicros: number; recoverableMicros: number; code: string; name: string; rate: string; recoverable: string; effectiveFrom: string; effectiveTo?: string | undefined; id: string; };
export type CommitCreateBudgetResult = { status: string; name: string; fiscalYear: number; currency: string; id: string; };
export type CommitCreateLeaseResult = { id: string; status: string; payment: string; };
export type CommitCreateLoanResult = { id: string; status: string; principal: string; };
export type CommitCreateRevenueContractResult = { id: string; status: string; transactionPrice: string; };
export type CommitCreateRevenueObligationResult = { id: string; status: string; allocated: string; };
