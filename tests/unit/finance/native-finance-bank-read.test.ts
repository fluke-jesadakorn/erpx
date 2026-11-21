import { beforeEach, describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));
vi.mock('next/headers', () => ({ headers: vi.fn(async () => new Headers({ host: 'acme.example.test', cookie: 'opaque' })) }));
import {
	createNativeFinanceBankHttpGet,
	createNativeFinanceBankRead,
	nativeFinanceBankPageFailure,
	nativeFinanceBankWorkspace,
	type NativeFinanceBankWorkspace,
} from '../../../packages/feature-finance/src/adapters/server/native-finance-bank-read';

const config = JSON.stringify({
	privateBaseUrl: 'http://127.0.0.1:44993',
	timeoutMs: 250,
	holdings: [{ origin: 'https://acme.example.test', holdingSlug: 'acme', holdingId: 'h001' }],
});
const actor = {
	userId: 'user',
	holdingId: 'h001',
	holdingSlug: 'acme',
	companyId: 'company',
	branchId: null,
	departmentId: null,
	assignmentId: 'assignment',
	role: 'finance',
	assignmentScope: 'holding',
	scope: { kind: 'holding' as const },
	authority: { revision: 7, assignmentVersion: 3 },
	modules: ['finance'],
	iam: { permissions: ['finance.read', 'finance.banking'] },
};
const statement = (id = 'statement-1') => ({
	id,
	company_id: 'company',
	bank_account: 'bank',
	statement_number: 'ST-1',
	statement_date: '2026-01-31',
	opening_minor: 0,
	closing_minor: 1500,
	closing_fx_rate_micros: 1000000,
	base_closing_minor: 1500,
	currency: 'THB',
	base_currency: 'THB',
	status: 'imported',
	source_hash: 'hash',
	version: 1,
	imported_by: 'user',
	created_at: 0,
	page_key: '2026-01-31',
});
const transaction = (id = 'transaction-1') => ({
	id,
	company_id: 'company',
	statement_id: 'statement-1',
	transaction_date: '2026-01-05',
	external_id: 'EXT-1',
	description: 'Deposit',
	amount_minor: 1500,
	currency: 'THB',
	match_status: 'unmatched',
	matched_type: null,
	matched_id: null,
	version: 1,
	bank_account: 'bank',
	statement_number: 'ST-1',
	page_key: '2026-01-05',
});
const payment = (id = 'payment-1') => ({
	id,
	company_id: 'company',
	kind: 'receipt',
	payment_number: 'RC-1',
	payment_date: '2026-01-05',
	currency: 'THB',
	amount_minor: 1500,
	outstanding_minor: 1500,
	bank_account: 'bank',
	status: 'released',
	journal_id: 'journal',
	version: 2,
	created_by: 'user',
	created_at: 0,
	updated_at: 0,
});
const account = () => ({
	id: 'bank',
	chart_id: 'chart',
	code: '1000',
	name: 'Bank',
	currency: 'THB',
	base_currency: 'THB',
});
const page = (items: unknown[]) => ({ items, nextCursor: null, previousCursor: null, hasMore: false });
const workspace = (): NativeFinanceBankWorkspace =>
	({
		accounts: [account()],
		statements: [statement()],
		statementPage: page([statement()]),
		transactions: [transaction()],
		transactionPage: page([transaction()]),
		payments: [payment()],
		selectedTransaction: transaction(),
	}) as unknown as NativeFinanceBankWorkspace;
const json = (value: unknown, status = 200) =>
	new Response(JSON.stringify(value), { status, headers: { 'content-type': 'application/json' } });
const incoming = () => new Headers({ host: 'acme.example.test', cookie: 'opaque' });

beforeEach(() => {
	delete process.env.ERPX_NATIVE_FINANCE_BANK_CONFIG;
});

describe('native Finance bank workspace transport', () => {
	it('reads the exact Company bank path with preserved query and work headers', async () => {
		const calls: { url: URL; headers: Headers }[] = [];
		const transport = vi.fn(async (url: URL, headers: Headers) => {
			calls.push({ url, headers });
			return json({ data: workspace() });
		});
		const read = createNativeFinanceBankRead(config, transport);
		const result = await read(incoming(), actor, 'acme', {
			print: '1',
			transactionCursor: 'transaction-cursor',
			statementCursor: 'statement-cursor',
			match: 'transaction-1',
			new: '1',
			chart: 'chart',
			view: 'unmatched',
			q: 'deposit',
		});
		expect(result.statements).toHaveLength(1);
		expect(result.selectedTransaction?.id).toBe('transaction-1');
		expect(calls).toHaveLength(1);
		expect(calls[0].url.pathname).toBe('/api/v1/holdings/acme/finance/bank');
		expect(Object.fromEntries(calls[0].url.searchParams)).toEqual({
			print: '1',
			transactionCursor: 'transaction-cursor',
			statementCursor: 'statement-cursor',
			match: 'transaction-1',
			new: '1',
			chart: 'chart',
			view: 'unmatched',
			q: 'deposit',
		});
		expect(calls[0].headers.get('host')).toBe('acme.example.test');
		expect(calls[0].headers.get('cookie')).toBe('opaque');
		expect(calls[0].headers.get('x-erpx-context-company')).toBe('company');
		expect(calls[0].headers.get('x-erpx-context-assignment')).toBe('assignment');
		expect(calls[0].headers.get('x-erpx-context-revision')).toBe('7');
		expect(calls[0].headers.get('x-erpx-context-assignment-version')).toBe('3');
	});

	it('rejects foreign rows and mismatched hosts instead of accepting a loose envelope', async () => {
		const foreign = workspace();
		foreign.statements[0] = { ...foreign.statements[0], company_id: 'foreign' };
		const read = createNativeFinanceBankRead(config, async () => json({ data: foreign }));
		await expect(read(incoming(), actor, 'acme', {})).rejects.toMatchObject({
			code: 'NATIVE_FINANCE_RESPONSE_INVALID',
		});
		const mismatch = createNativeFinanceBankRead(config, async () => json({ data: workspace() }));
		await expect(
			mismatch(new Headers({ host: 'northstar.example.test' }), actor, 'acme', {}),
		).rejects.toMatchObject({ code: 'NATIVE_FINANCE_AUTHORITY_INVALID' });
	});

	it('never falls back after a configured native failure', async () => {
		const read = createNativeFinanceBankRead(config, async () => json({}, 403));
		await expect(read(incoming(), actor, 'acme', {})).rejects.toMatchObject({
			code: 'NATIVE_FINANCE_UNAVAILABLE',
			status: 403,
		});
		const missing = createNativeFinanceBankRead(config, async () =>
			json({ error: { code: 'BANK_TRANSACTION_NOT_FOUND', message: 'Bank movement not found.' } }, 404),
		);
		await expect(missing(incoming(), actor, 'acme', { match: 'missing' })).rejects.toMatchObject({
			code: 'BANK_TRANSACTION_NOT_FOUND',
			status: 404,
		});
		expect(nativeFinanceBankPageFailure(await missing(incoming(), actor, 'acme', { match: 'missing' }).catch((error) => error))?.status).toBe(404);
		process.env.ERPX_NATIVE_FINANCE_BANK_CONFIG = config;
		await expect(nativeFinanceBankWorkspace(actor, 'acme', {})).rejects.toMatchObject({
			code: 'NATIVE_FINANCE_UNAVAILABLE',
		});
		delete process.env.ERPX_NATIVE_FINANCE_BANK_CONFIG;
		await expect(nativeFinanceBankWorkspace(actor, 'acme', {})).resolves.toBeUndefined();
	});

	it('serves the bounded GET envelope and maps configured failures', async () => {
		const get = createNativeFinanceBankHttpGet(config, async () => json({ data: workspace() }));
		const request = new Request('https://acme.example.test/api/v1/holdings/acme/finance/bank?chart=chart', {
			headers: {
				host: 'acme.example.test',
				cookie: 'opaque',
				'x-erpx-context-company': 'company',
				'x-erpx-context-assignment': 'assignment',
				'x-erpx-context-revision': '7',
				'x-erpx-context-assignment-version': '3',
			},
		});
		const response = await get(request, actor, 'acme');
		expect(response.status).toBe(200);
		expect(response.headers.get('cache-control')).toBe('private, no-store');
		expect(response.headers.get('referrer-policy')).toBe('no-referrer');
		expect(response.headers.get('x-content-type-options')).toBe('nosniff');
		expect((await response.json()).data.statements).toHaveLength(1);

		const failed = createNativeFinanceBankHttpGet(config, async () =>
			json({ error: { code: 'BANK_TRANSACTION_NOT_FOUND', message: 'Bank movement not found.' } }, 404),
		);
		const missing = await failed(request, actor, 'acme');
		expect(missing.status).toBe(404);
		expect((await missing.json()).error.code).toBe('BANK_TRANSACTION_NOT_FOUND');

		const invalid = await get(
			new Request('https://acme.example.test/api/v1/holdings/acme/finance/bank?unexpected=1', {
				headers: { host: 'acme.example.test' },
			}),
			actor,
			'acme',
		);
		expect(invalid.status).toBe(422);
		expect((await invalid.json()).error.code).toBe('NATIVE_FINANCE_QUERY_INVALID');
	});
});
