import { samePerson } from "@/lib/identity_access/iam";
import { decimalToMinor, minorToDecimal } from "@/lib/finance/finance-money";
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import type { FinanceError } from '../domain/invoice-line-revision';
import type { JournalInput } from '../domain/finance';
import type { SnapshotInput } from '../domain/company-money';
import { createAssetValuePolicy, type Asset, type Disposal, type AssetInput, type AssetEdit, type AssetActivation, type AssetDepreciation, type AssetDecision, type AssetDisposal, type AssetRow, type AssetAccount, type AssetPolicyDisposal, type AssetPolicyDisposalInput } from '../domain/assets';
type Common<Input> = {
    actor: WorkContext;
    id: string;
    input: Input;
    key: string;
    now: number;
};
type Result = {
    id: string;
    status: string;
    version: number;
    [key: string]: unknown;
};
type AssetCommon<Input> = Common<Input> & {
    asset: Asset;
    result: Result;
};
type Accounts = {
    cost: string;
    accumulated?: string | null;
    expense?: string | null;
};
type DisposalResult = Result & {
    assetVersion: number;
};
export interface AssetPorts {
    error: FinanceError;
    permission(actor: WorkContext, permission: 'finance.read' | 'finance.write' | 'finance.approve'): WorkContext;
    ready(actor: WorkContext): Promise<void>;
    transaction<Value>(actor: WorkContext, work: () => Promise<Value>): Promise<Value>;
    now(): number;
    id(): string;
    hash(input: string): Promise<string>;
    replay(actor: WorkContext, key: string, op: string, id: string, input: unknown): Promise<Record<string,unknown>|null>;
    asset(actor: WorkContext, id: string): Promise<Asset | null>;
    accountRows(actor: WorkContext, ids: Accounts): Promise<AssetAccount[]>;
    secondaryChart(actor: WorkContext, chartId: string): Promise<boolean>;
    events(actor: WorkContext, id: string): Promise<AssetRow[]>;
    depreciations(actor: WorkContext, id: string): Promise<AssetRow[]>;
    disposals(actor: WorkContext, id: string): Promise<AssetRow[]>;
    page(actor: WorkContext): Promise<AssetRow[]>;
    options(actor: WorkContext): Promise<{
        accounts: AssetRow[];
        periods: AssetRow[];
        dimensions: AssetRow[];
        dimensionValues: AssetRow[];
    }>;
    reviewedPolicy(actor: WorkContext, id: string): Promise<boolean>;
    pendingDisposal(actor: WorkContext, id: string): Promise<boolean>;
    depreciationHistory(actor: WorkContext, id: string): Promise<{
        ordinal: number;
        amount_minor: number;
    }[]>;
    latestDepreciation(actor: WorkContext, id: string): Promise<string | null>;
    disposalAccounts(actor: WorkContext, chart: string, input: AssetDisposal): Promise<{
        id: string;
        category: string;
    }[]>;
    disposal(actor: WorkContext, id: string): Promise<Disposal | null>;
    retainedPolicyHash(actor: WorkContext, id: string): Promise<string | null>;
    preparePolicyDisposal(actor: WorkContext, assetId: string, input: AssetPolicyDisposalInput, key: string, status: 'draft' | 'posted'): Promise<AssetPolicyDisposal>;
    prepareJournal(actor: WorkContext, input: JournalInput, key: string): Promise<{
        result: {
            id: string;
        };
    }>;
    prepareMoney(actor: WorkContext, input: SnapshotInput): Promise<void>;
    prepareDecision(actor: WorkContext, id: string, key: string, asset: Asset, disposal: Disposal): Promise<void>;
    commitCreate(facts: Common<AssetInput> & {
        amount: {
            cost: number;
            residual: number;
        };
        result: Result;
    }): Promise<Record<string,unknown>>;
    commitEdit(facts: AssetCommon<AssetEdit> & {
        amount: {
            cost: number;
            residual: number;
        };
        next: number;
    }): Promise<Record<string,unknown>>;
    commitCancel(facts: AssetCommon<AssetDecision>): Promise<Record<string,unknown>>;
    commitActivate(facts: AssetCommon<AssetActivation> & {
        next: number;
        journalId: string;
        decisionId: string;
        decisionHash: string;
    }): Promise<Record<string,unknown>>;
    commitDepreciation(facts: AssetCommon<AssetDepreciation> & {
        next: number;
        index: number;
        amount: number;
        target: bigint;
        journalId: string;
    }): Promise<Record<string,unknown>>;
    commitPrepareDisposal(facts: AssetCommon<AssetDisposal> & {
        disposalId: string;
        amount: number;
        policy: AssetPolicyDisposal;
    }): Promise<Record<string,unknown>>;
    commitDisposal(facts: AssetCommon<AssetDecision> & {
        result: DisposalResult;
        action: 'post' | 'cancel';
        operation: string;
        disposal: Disposal;
        journalId: string | null;
        secondaryJournalIds: string[];
        decisionId: string;
        decisionHash: string;
    }): Promise<Record<string,unknown>>;
}
export function createAssets(ports: AssetPorts) {
    const { version, values, monthOrdinal } = createAssetValuePolicy(ports.error);
    async function resources(actor: WorkContext, permission: 'finance.read' | 'finance.write' | 'finance.approve') { actor = ports.permission(actor, permission); await ports.ready(actor); return { actor }; }
    async function getAsset(actor: WorkContext, id: string) { const asset = await ports.asset(actor, id); if (!asset)
        throw ports.error(404, 'ASSET_NOT_FOUND', 'Asset not found.'); return asset; }
    async function accounts(actor: WorkContext, ids: Accounts) {
        const rows = await ports.accountRows(actor, ids), byId = new Map(rows.map(row => [row.id, row])), cost = byId.get(ids.cost);
        if (!cost || cost.category !== 'asset' || [ids.accumulated, ids.expense].filter(Boolean).some(id => !byId.has(id!) || byId.get(id!)!.chart_id !== cost.chart_id))
            throw ports.error(422, 'ACCOUNT_INVALID', 'Asset accounts must be active and belong to one company chart.');
        if (ids.accumulated && (ids.accumulated === ids.cost || byId.get(ids.accumulated)?.category !== 'asset'))
            throw ports.error(422, 'ASSET_ACCUMULATED_ACCOUNT_REQUIRED', 'Use a separate accumulated depreciation asset account.');
        if (ids.expense && (ids.expense === ids.cost || ids.expense === ids.accumulated || byId.get(ids.expense)?.category !== 'expense'))
            throw ports.error(422, 'ASSET_EXPENSE_ACCOUNT_REQUIRED', 'Use a separate depreciation expense account.');
        if (await ports.secondaryChart(actor, cost.chart_id))
            throw ports.error(422, 'ASSET_PRIMARY_BOOK_REQUIRED', 'Prepare the business asset in the primary book; approved book policies retain its secondary carrying schedules.');
        return cost.chart_id;
    }
    async function assetDetail(actor: WorkContext, id: string) { const r = await resources(actor, 'finance.read'), asset = await getAsset(r.actor, id); const [events, depreciations, disposals] = await Promise.all([ports.events(r.actor, id), ports.depreciations(r.actor, id), ports.disposals(r.actor, id)]); return { asset, events, depreciations, disposals }; }
    async function assetPage(actor: WorkContext) { return ports.page((await resources(actor, 'finance.read')).actor); }
    async function assetOptions(actor: WorkContext) { return ports.options((await resources(actor, 'finance.read')).actor); }
    async function createAsset(actor: WorkContext, input: AssetInput, key: string) {
        actor = (await resources(actor, 'finance.write')).actor;
        const id = `asset-${key}`, prior = await ports.replay(actor, key, 'finance.asset.create', id, input);
        if (prior)
            return prior;
        const amount = values(input);
        await accounts(actor, { cost: input.accountId, accumulated: input.depreciationAccountId, expense: input.depreciationExpenseAccountId });
        await ports.prepareMoney(actor, { sourceType: 'finance_asset', sourceId: id, amountKind: 'cost', transactionDate: input.inServiceDate, currency: input.currency, currencyScale: 2, amountUnits: String(amount.cost) });
        const result = { id, status: 'draft', version: 1, cost: minorToDecimal(amount.cost) };
        return ports.commitCreate({ actor, input, key, id, amount, now: ports.now(), result });
    }
    async function editAsset(actor: WorkContext, id: string, input: AssetEdit, key: string) { actor = (await resources(actor, 'finance.write')).actor; const prior = await ports.replay(actor, key, 'finance.asset.edit', id, input); if (prior)
        return prior; const asset = await getAsset(actor, id); version(asset, input.expectedVersion); if (asset.status !== 'draft')
        throw ports.error(409, 'ASSET_NOT_DRAFT', 'Only a draft asset can be edited.'); const amount = values(input); await accounts(actor, { cost: input.accountId, accumulated: input.depreciationAccountId, expense: input.depreciationExpenseAccountId }); const next = asset.version + 1; await ports.prepareMoney(actor, { sourceType: 'finance_asset_revision', sourceId: `${id}:${next}`, amountKind: 'cost', transactionDate: input.inServiceDate, currency: input.currency, currencyScale: 2, amountUnits: String(amount.cost) }); return ports.commitEdit({ actor, input, key, id, asset, amount, next, now: ports.now(), result: { id, status: 'draft', version: next, cost: minorToDecimal(amount.cost) } }); }
    async function cancelDraftAsset(actor: WorkContext, id: string, input: AssetDecision, key: string) { actor = (await resources(actor, 'finance.write')).actor; const prior = await ports.replay(actor, key, 'finance.asset.cancel', id, input); if (prior)
        return prior; const asset = await getAsset(actor, id); version(asset, input.expectedVersion); if (asset.status !== 'draft')
        throw ports.error(409, 'ASSET_NOT_DRAFT', 'Only an unrecognized draft asset can be cancelled.'); return ports.commitCancel({ actor, input, key, id, asset, now: ports.now(), result: { id, status: 'retired', version: asset.version + 1, cancelled: true } }); }
    async function activateAsset(actor: WorkContext, id: string, input: AssetActivation, key: string) {
        actor = (await resources(actor, 'finance.approve')).actor;
        const prior = await ports.replay(actor, key, 'finance.asset.activate', id, input);
        if (prior)
            return prior;
        const asset = await getAsset(actor, id);
        version(asset, input.expectedVersion);
        if (asset.status !== 'draft' || asset.activation_journal_id)
            throw ports.error(409, 'ASSET_NOT_DRAFT', 'Only an unrecognized draft asset can be activated.');
        if (samePerson(actor, asset.created_by))
            throw ports.error(403, 'SELF_APPROVAL', 'Another finance reviewer must recognize the asset.');
        if (!asset.depreciation_account_id || !asset.depreciation_expense_account_id)
            throw ports.error(422, 'ASSET_DEPRECIATION_MAPPING_REQUIRED', 'Configure accumulated depreciation and expense accounts before recognition.');
        if (input.entryDate > asset.in_service_date)
            throw ports.error(422, 'ASSET_RECOGNITION_DATE', 'Recognize acquisition on or before its in-service date.');
        const chart = await accounts(actor, { cost: asset.account_id, accumulated: asset.depreciation_account_id, expense: asset.depreciation_expense_account_id });
        if ([asset.account_id, asset.depreciation_account_id, asset.depreciation_expense_account_id].includes(input.offsetAccountId))
            throw ports.error(422, 'ASSET_OFFSET_INVALID', 'Use a separate acquisition offset account.');
        const journal = await ports.prepareJournal(actor, { chartId: chart, periodId: input.periodId, entryDate: input.entryDate, currency: asset.currency, memo: `Acquisition ${asset.asset_number}: ${input.evidenceRef}`, sourceType: 'asset_acquisition', sourceId: id, isOpening: false, lines: [{ accountId: asset.account_id, debit: minorToDecimal(asset.cost_minor), credit: '0', dimensions: input.dimensions }, { accountId: input.offsetAccountId, debit: '0', credit: minorToDecimal(asset.cost_minor), dimensions: input.dimensions }] }, key), next = asset.version + 1, result = { id, status: 'in_service', version: next, journalId: journal.result.id };
        return ports.commitActivate({ actor, input, key, id, asset, next, now: ports.now(), result, journalId: journal.result.id, decisionId: ports.id(), decisionHash: await ports.hash(JSON.stringify({ asset, input })) });
    }
    async function depreciateAsset(actor: WorkContext, id: string, input: AssetDepreciation, key: string) { actor = (await resources(actor, 'finance.write')).actor; const prior = await ports.replay(actor, key, 'finance.asset.depreciate', id, input); if (prior)
        return prior; if (await ports.reviewedPolicy(actor, id))
        throw ports.error(409, 'ASSET_POLICY_OWNER_REQUIRED', "Prepare and independently approve depreciation through the asset's reviewed book policies."); const asset = await getAsset(actor, id); version(asset, input.expectedVersion); if (asset.status !== 'in_service' || !asset.activation_journal_id)
        throw ports.error(409, 'ASSET_NOT_RECOGNIZED', 'Recognize the asset before depreciation.'); if (!asset.depreciation_account_id || !asset.depreciation_expense_account_id)
        throw ports.error(422, 'ASSET_DEPRECIATION_MAPPING_REQUIRED', 'Configure accumulated depreciation and expense accounts.'); if (await ports.pendingDisposal(actor, id))
        throw ports.error(409, 'ASSET_DISPOSAL_PENDING', 'Decide the prepared disposal before changing depreciation.'); const priorRows = await ports.depreciationHistory(actor, id), index = priorRows.length; if (index >= asset.useful_life_months)
        throw ports.error(409, 'ASSET_FULLY_DEPRECIATED', 'The scheduled depreciation is complete.'); if (monthOrdinal(input.entryDate) !== monthOrdinal(asset.in_service_date) + index || input.entryDate < asset.in_service_date)
        throw ports.error(422, 'ASSET_DEPRECIATION_SEQUENCE', 'Post each in-service calendar month once, in sequence.'); const depreciable = BigInt(asset.cost_minor - asset.residual_minor), life = BigInt(asset.useful_life_months), previous = depreciable * BigInt(index) / life, target = depreciable * BigInt(index + 1) / life, amount = Number(target - previous); if (asset.accumulated_depreciation_minor !== Number(previous) || priorRows.reduce((sum, row) => sum + row.amount_minor, 0) !== Number(previous))
        throw ports.error(409, 'ASSET_HISTORY_RECONCILIATION_REQUIRED', 'The retained asset history needs reconciliation before another depreciation.'); if (amount <= 0)
        throw ports.error(422, 'ASSET_DEPRECIATION_PRECISION', 'Cost is too small for a positive monthly posting under this convention.'); const chart = await accounts(actor, { cost: asset.account_id, accumulated: asset.depreciation_account_id, expense: asset.depreciation_expense_account_id }), journal = await ports.prepareJournal(actor, { chartId: chart, periodId: input.periodId, entryDate: input.entryDate, currency: asset.currency, memo: `Full-month straight-line depreciation ${asset.asset_number}`, sourceType: 'asset_depreciation', sourceId: `${id}:${input.entryDate.slice(0, 7)}`, isOpening: false, lines: [{ accountId: asset.depreciation_expense_account_id, debit: minorToDecimal(amount), credit: '0', dimensions: input.dimensions }, { accountId: asset.depreciation_account_id, debit: '0', credit: minorToDecimal(amount), dimensions: input.dimensions }] }, key), next = asset.version + 1, result = { id, status: 'posted', version: next, journalId: journal.result.id, amount: minorToDecimal(amount) }; await ports.prepareMoney(actor, { sourceType: 'asset_depreciation', sourceId: `${id}:${input.entryDate.slice(0, 7)}`, amountKind: 'depreciation', transactionDate: input.entryDate, currency: asset.currency, currencyScale: 2, amountUnits: String(amount) }); return ports.commitDepreciation({ actor, input, key, id, asset, next, index, amount, target, now: ports.now(), result, journalId: journal.result.id }); }
    async function prepareAssetDisposal(actor: WorkContext, id: string, input: AssetDisposal, key: string) { actor = (await resources(actor, 'finance.write')).actor; return ports.transaction(actor, async () => { const disposalId = `asset-disposal-${key}`, prior = await ports.replay(actor, key, 'finance.asset.disposal.prepare', disposalId, input); if (prior)
        return prior; const asset = await getAsset(actor, id); version(asset, input.expectedVersion); if (asset.status !== 'in_service' || !asset.activation_journal_id)
        throw ports.error(409, 'ASSET_NOT_RECOGNIZED', 'Only a recognized in-service asset can be disposed.'); const amount = decimalToMinor(input.proceeds); if (amount < 0 || amount > 0 && !input.proceedsAccountId)
        throw ports.error(422, 'ASSET_PROCEEDS_INVALID', 'Positive disposal proceeds require a destination account.'); if (input.entryDate < asset.in_service_date)
        throw ports.error(422, 'ASSET_DISPOSAL_DATE', 'Disposal cannot precede the in-service date.'); const latest = await ports.latestDepreciation(actor, id); if (latest && input.entryDate < latest)
        throw ports.error(422, 'ASSET_DISPOSAL_DATE', 'Disposal cannot precede posted depreciation.'); const chart = await accounts(actor, { cost: asset.account_id, accumulated: asset.depreciation_account_id, expense: asset.depreciation_expense_account_id }), mapped = await ports.disposalAccounts(actor, chart, input), byId = new Map(mapped.map(a => [a.id, a.category])); if (!['revenue', 'other_income'].includes(byId.get(input.gainAccountId) ?? '') || !['expense', 'other_expense'].includes(byId.get(input.lossAccountId) ?? '') || amount > 0 && byId.get(input.proceedsAccountId!) !== 'asset')
        throw ports.error(422, 'ASSET_DISPOSAL_ACCOUNTS_INVALID', 'Use same-chart asset proceeds, income gain and expense loss accounts.'); const policy = await ports.preparePolicyDisposal(actor, id, { id: disposalId, periodId: input.periodId, entryDate: input.entryDate, proceeds: amount, gainAccountId: input.gainAccountId, lossAccountId: input.lossAccountId }, key, 'draft'); return ports.commitPrepareDisposal({ actor, input, key, id, asset, disposalId, amount, now: ports.now(), result: { id: disposalId, assetId: id, status: 'draft', version: 1 }, policy }); }); }
    async function decideAssetDisposal(actor: WorkContext, id: string, action: 'post' | 'cancel', input: AssetDecision, key: string) {
        actor = (await resources(actor, action === 'post' ? 'finance.approve' : 'finance.write')).actor;
        return ports.transaction(actor, async () => {
            const operation = `finance.asset.disposal.${action}`, prior = await ports.replay(actor, key, operation, id, input);
            if (prior)
                return prior;
            const disposal = await ports.disposal(actor, id);
            if (!disposal)
                throw ports.error(404, 'ASSET_DISPOSAL_NOT_FOUND', 'Disposal not found.');
            version(disposal, input.expectedVersion);
            if (disposal.status !== 'draft')
                throw ports.error(409, 'ASSET_DISPOSAL_FINAL', 'This disposal was already decided.');
            const asset = await getAsset(actor, disposal.asset_id);
            version(asset, disposal.asset_version);
            if (action === 'post' && samePerson(actor, disposal.created_by))
                throw ports.error(403, 'SELF_APPROVAL', 'Another finance reviewer must post the disposal.');
            await ports.prepareDecision(actor, id, key, asset, disposal);
            let journalId: string | null = null, secondaryJournalIds: string[] = [];
            if (action === 'post') {
                if (asset.status !== 'in_service' || !asset.activation_journal_id || !asset.depreciation_account_id)
                    throw ports.error(409, 'ASSET_NOT_RECOGNIZED', 'The current asset is unavailable for disposal.');
                const book = asset.cost_minor - asset.accumulated_depreciation_minor, gain = disposal.proceeds_minor - book, chart = await accounts(actor, { cost: asset.account_id, accumulated: asset.depreciation_account_id, expense: asset.depreciation_expense_account_id }), retained = await ports.retainedPolicyHash(actor, id), policy = await ports.preparePolicyDisposal(actor, asset.id, { id, periodId: disposal.period_id, entryDate: disposal.entry_date, proceeds: disposal.proceeds_minor, gainAccountId: disposal.gain_account_id, lossAccountId: disposal.loss_account_id }, key, 'posted');
                if (policy.hash !== retained)
                    throw ports.error(409, 'ASSET_DISPOSAL_POLICY_CHANGED', 'Book carrying or policy evidence changed. Cancel and prepare a fresh disposal.');
                secondaryJournalIds = policy.journalIds;
                const lines = [{ accountId: asset.account_id, debit: '0', credit: minorToDecimal(asset.cost_minor) }, ...(asset.accumulated_depreciation_minor > 0 ? [{ accountId: asset.depreciation_account_id, debit: minorToDecimal(asset.accumulated_depreciation_minor), credit: '0' }] : []), ...(disposal.proceeds_minor > 0 ? [{ accountId: disposal.proceeds_account_id!, debit: minorToDecimal(disposal.proceeds_minor), credit: '0' }] : []), ...(gain > 0 ? [{ accountId: disposal.gain_account_id, debit: '0', credit: minorToDecimal(gain) }] : gain < 0 ? [{ accountId: disposal.loss_account_id, debit: minorToDecimal(-gain), credit: '0' }] : [])];
                const journal = await ports.prepareJournal(actor, { chartId: chart, periodId: disposal.period_id, entryDate: disposal.entry_date, currency: asset.currency, memo: `Disposal ${asset.asset_number}: ${input.reason}`, sourceType: 'asset_disposal', sourceId: id, isOpening: false, lines: lines.map(l => ({ ...l, dimensions: policy.dimensions })) }, key);
                journalId = journal.result.id;
                await ports.prepareMoney(actor, { sourceType: 'asset_disposal', sourceId: id, amountKind: 'proceeds', transactionDate: disposal.entry_date, currency: asset.currency, currencyScale: 2, amountUnits: String(disposal.proceeds_minor) });
            }
            const result = { id, assetId: asset.id, status: action === 'post' ? 'posted' : 'cancelled', version: disposal.version + 1, journalId, secondaryJournalIds, assetVersion: asset.version + (action === 'post' ? 1 : 0) };
            return ports.commitDisposal({ actor, id, action, input, key, operation, disposal, asset, journalId, secondaryJournalIds, now: ports.now(), result, decisionId: ports.id(), decisionHash: await ports.hash(JSON.stringify({ disposal, asset, input })) });
        });
    }
    return { assetDetail, assetPage, assetOptions, createAsset, editAsset, cancelDraftAsset, activateAsset, depreciateAsset, prepareAssetDisposal, decideAssetDisposal };
}

