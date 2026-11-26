import {existsSync,readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import ts from 'typescript';
import {z} from 'zod';
import {describe,it,expect,vi} from 'vitest';

/**
 * G4 cutover for the generic stock-count transition catch-all branch.
 *
 * The Rust Axum handler in `inventory_count_http.rs` already executes the
 * owner SQL for `POST /api/v1/holdings/{holding_slug}/stock-counts/{id}/{action}`.
 * This suite proves the Next catch-all no longer prepares, proxies or falls
 * back for that operation, and pins the Rust payload/response contract that
 * replaces it. No SQL-plan/`include_str!` fragment is accepted as execution.
 */

const ROUTE = 'packages/workplace/src/routes/api/v1/holdings/[holdingSlug]/stock-counts/[[...path]]/route.ts';
const RUST_HTTP = 'src/composition/src/inventory_count_http.rs';
const RUST_COMMANDS = 'src/composition/src/inventory_count_commands.rs';
const RUST_SQL = 'packages/server-inventory/src/adapters/counts.rs';
const TS_CLIENT = 'packages/feature-inventory/src/lib/native-stock-count-command.ts';

/**
 * Exact deletion packet for the generic transition family. The catch-all still
 * serves list/detail/options reads and the initial-physical families, so only
 * this operation's branch is retired today; every path below is the authority
 * that must disappear when the whole family is split and deleted.
 */
const SHARED_HANDLER = ROUTE;
const APP_REEXPORTS = [
  'apps/compat/src/app/api/v1/holdings/[holdingSlug]/stock-counts/[[...path]]/route.ts',
  'apps/sales/src/app/api/v1/holdings/[holdingSlug]/stock-counts/[[...path]]/route.ts',
  'apps/supply/src/app/api/v1/holdings/[holdingSlug]/stock-counts/[[...path]]/route.ts',
];
const CALLERS_TO_MOVE = [
  'packages/feature-inventory/src/components/use-count-command.ts',
];
/** Retained legacy: the helper is still authoritative for two other route
 * families outside this claim. Its owner functions are NOT cut over here. */
const RETAINED_TS_AUTHORITY = [
  {
    path: 'packages/feature-inventory/src/lib/native-stock-count-command.ts:311',
    consumers: [
      'packages/feature-pos/src/routes/api/v1/holdings/[holdingSlug]/pos/initial-physical-stock/[[...path]]/route.ts:5,29',
      'packages/feature-manufacturing/src/routes/api/v1/holdings/[holdingSlug]/production/initial-physical-stock/[[...path]]/route.ts:5,31',
    ],
    testOracle: 'tests/unit/inventory/initial-physical-stock-client.test.ts:167',
    removalTaskIds: ['migrate.pos', 'migrate.manufacturing'],
    removalCondition:
      'POS and Manufacturing initial-physical action routes must call the Rust owner endpoint through a non-business Host/session-preserving transport (or be deleted after their own native cutover); then delete the helper and its test-oracle port and only then delete the shared catch-all and its three app re-exports.',
  },
];

const read = (path: string) => readFileSync(resolve(process.cwd(), path), 'utf8');

/** Execute the real GET/POST handlers from the route source with closed ports. */
function controller(owner: 'service' | 'other' = 'other') {
  const source = read(ROUTE);
  const file = ts.createSourceFile('route.ts', source, ts.ScriptTarget.Latest, true);
  const nodes = file.statements.filter(
    (node) => ts.isFunctionDeclaration(node) && node.name && ['GET', 'POST'].includes(node.name.text),
  );
  expect(nodes).toHaveLength(2);
  const code = ts.transpileModule(
    nodes.map((node) => node.getText(file)).join('\n').replaceAll('export async function', 'async function'),
    {compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None}},
  ).outputText;

  class ApiError extends Error {
    constructor(readonly status: number, readonly code: string, message: string) {
      super(message);
    }
  }
  const actor = {companyId: 'company-1', assignmentId: 'assignment-1'};
  const ownerPort = () => {
    throw Error('OWNER_EXECUTION_REACHED');
  };
  const parse = vi.fn((value: unknown) => value);
  const nativeSave = vi.fn(async () => Response.json({data: {id: 'count-1', status: 'draft', version: 1}}));
  const serviceCommand = vi.fn(async () => ({data: {id: 'count-1', status: 'submitted', version: 2}}));
  const ports = {
    Response,
    URL,
    z,
    ApiError,
    errorResponse: (error: {status?: number; code?: string; message?: string}) =>
      Response.json({error: {code: error.code, message: error.message}}, {status: error.status ?? 500}),
    requireWorkContext: vi.fn(async () => actor),
    requestPublicOrigin: (request: Request) => new URL(request.url).origin,
    canonicalDestinationLinks: (value: unknown) => value,
    serviceCountWriterOwner: vi.fn(async () => owner),
    serviceCountCommand: serviceCommand,
    serviceCountRead: ownerPort,
    initialPhysicalStockCountWorkspace: ownerPort,
    initialPhysicalStockSeed: ownerPort,
    transitionInitialPhysicalStockCount: ownerPort,
    initialPhysicalStockCountFlow: ownerPort,
    assignInitialPhysicalStockCountFlow: ownerPort,
    initialPhysicalStockCountSchema: {parse},
    initialPhysicalStockCountActionSchema: {parse},
    initialPhysicalStockCountFlowSchema: {parse},
    countSchema: {parse},
    listCounts: ownerPort,
    countOptions: ownerPort,
    countDetail: ownerPort,
    nativeStockCountSave: nativeSave,
  };
  const handlers = new Function(...Object.keys(ports), code + '\nreturn {GET,POST};')(
    ...Object.values(ports),
  ) as Record<'GET' | 'POST', (request: Request, context: {params: Promise<{holdingSlug: string; path: string[]}>}) => Promise<Response>>;
  const send = (path: string[]) =>
    handlers.POST(
      new Request('https://acme.jesadakorn.com/api/v1/holdings/acme/stock-counts/' + path.join('/'), {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Idempotency-Key': 'native-transition-key',
          Origin: 'https://acme.jesadakorn.com',
        },
        body: JSON.stringify({expectedVersion: 0, reason: 'Reviewed stock count'}),
      }),
      {params: Promise.resolve({holdingSlug: 'acme', path})},
    );
  return {send, parse, nativeSave, serviceCommand};
}

describe('generic stock-count transition removed from the Next catch-all', () => {
  it('answers a generic {id}/{action} command with 404 and no owner/SQL execution', async () => {
    for (const action of ['submit', 'reject', 'cancel', 'approve', 'post']) {
      const c = controller('other');
      const response = await c.send(['count-1', action]);
      expect(response.status).toBe(404);
      expect(c.parse).not.toHaveBeenCalled();
      expect(c.nativeSave).not.toHaveBeenCalled();
      expect(c.serviceCommand).not.toHaveBeenCalled();
    }
  });

  it('leaves the Service-owned transition and the generic create/save branch intact', async () => {
    const service = controller('service');
    const response = await service.send(['count-1', 'approve']);
    expect(response.status).toBe(200);
    expect(service.serviceCommand).toHaveBeenCalledTimes(1);
    expect(await response.json()).toEqual({data: {id: 'count-1', status: 'submitted', version: 2}});

    const save = controller('other');
    const created = await save.send([]);
    expect(created.status).toBe(200);
    expect(save.nativeSave).toHaveBeenCalledTimes(1);
  });
});

describe('source assertion: replaced branch holds no SQL or owner execution', () => {
  const src = read(ROUTE);

  it('no longer imports or calls the generic transition proxy or its schema', () => {
    expect(src).not.toContain('nativeStockCountTransition');
    expect(src).not.toContain('countActionSchema');
  });

  it('contains no SQL text and no owner application for the generic transition', () => {
    expect(src).not.toMatch(/\b(SELECT|INSERT|UPDATE|DELETE)\s+[a-z_]/i);
    expect(src).not.toContain('inventoryCountApplication');
    expect(src).not.toContain('apply_count_transition');
    // The remaining native branches belong to other, untouched operations.
    expect(src).toContain('nativeStockCountSave');
    expect(src).toContain('serviceCountCommand');
    expect(src).toContain('initialPhysicalStockCountWorkspace');
  });
});

describe('Rust payload parity for the migrated transition', () => {
  const http = read(RUST_HTTP);
  const commands = read(RUST_COMMANDS);
  const sql = read(RUST_SQL);
  const client = read(TS_CLIENT);

  it('runs the exact private route through an Axum handler that executes owner SQL', () => {
    expect(http).toContain('"/api/v1/holdings/{holding_slug}/stock-counts/{id}/{action}"');
    expect(http).toContain('post(transition_count::<C>)');
    expect(http).toMatch(/target\s*\.transition\(/);
    expect(sql).toContain('apply_count_transition(');
    expect(sql).toContain('UPDATE inventory_counts SET status=$1');
    // A returned SQL plan is not execution: the handler must not emit one.
    expect(http).not.toContain('"sql"');
  });

  it('pins the header/context contract the lead must preserve on the public mount', () => {
    expect(http).toContain('unique_request_header(headers, "idempotency-key")');
    expect(http).toContain('parse_inbound_account_request(&state.policy, &headers)');
    expect(http).toContain('state.scopes.slugs(&headers)');
    expect(http).toContain('origin != format!("https://{host}")');
  });

  it('returns the same {data} envelope and result fields the TS client accepted', () => {
    expect(http).toContain('Json(json!({"data": data}))');
    expect(commands).toContain('CommandJson::from_serializable(&CountTransitionResult');
    expect(commands).toContain('CountTransitionResult {');
    expect(commands).toContain('status: action_status(self.action).into()');
    expect(client).toContain('data.id !== id');
    expect(client).toContain('data.status');
    expect(client).toContain('data.version');
  });

  it('pins the exact wire result/input shape, not just a SQL string', () => {
    const body = (name: string) =>
      commands.match(new RegExp(`struct ${name} \\{([\\s\\S]*?)\\n\\}`))?.[1] ?? '';
    const fields = (text: string) =>
      text
        .split('\n')
        .map((line) => line.trim())
        .filter((line) => line.includes(':'))
        .map((line) => line.replace(/^pub\s+/, '').split(':')[0]);
    const result = body('CountTransitionResult');
    expect(fields(result)).toEqual(['id', 'status', 'version', 'journal_id']);
    expect(result).toContain('#[serde(rename = "journalId")]');
    const input = body('CountTransitionInput');
    expect(fields(input)).toEqual(['expected_version', 'attachment_set_ids', 'reason']);
    expect(commands).toContain('#[serde(rename_all = "camelCase")]\npub struct CountTransitionInput');
    const mapping = commands.match(/fn action_status\(action: CountAction\) -> &'static str \{([\s\S]*?)\n\}/)?.[1] ?? '';
    for (const [action, status] of [
      ['Submit', 'submitted'],
      ['Approve', 'approved'],
      ['Post', 'posted'],
      ['Reject', 'rejected'],
      ['Cancel', 'cancelled'],
    ]) {
      expect(mapping).toContain(`CountAction::${action} => "${status}"`);
      expect(
        z
          .object({
            id: z.string(),
            status: z.enum(['submitted', 'approved', 'posted', 'rejected', 'cancelled']),
            version: z.number().int().min(1),
            journalId: z.string().nullish(),
          })
          .parse({id: 'count-1', status, version: 2, journalId: null}),
      ).toEqual({id: 'count-1', status, version: 2, journalId: null});
    }
  });
});

describe('exact route-deletion packet for the generic transition branch', () => {
  it('names only paths that exist and app re-exports of this shared handler', () => {
    for (const path of [
      SHARED_HANDLER,
      ...APP_REEXPORTS,
      ...CALLERS_TO_MOVE,
      RETAINED_TS_AUTHORITY[0]!.path.split(':')[0]!,
      ...RETAINED_TS_AUTHORITY[0]!.consumers.map((entry) => entry.split(':')[0]!),
      RETAINED_TS_AUTHORITY[0]!.testOracle.split(':')[0]!,
    ]) {
      expect(existsSync(resolve(process.cwd(), path)), path).toBe(true);
    }
    for (const path of APP_REEXPORTS) {
      expect(read(path)).toContain(
        'export { GET, POST } from "@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/stock-counts/[[...path]]/route"',
      );
    }
    expect(read(CALLERS_TO_MOVE[0]!)).toContain('`/api/v1${base}/stock-counts${attempt.path}`');
  });

  it('keeps the catch-all families that are not deleted with this branch', () => {
    const src = read(SHARED_HANDLER);
    for (const retained of [
      'listCounts',
      'countOptions',
      'countDetail',
      'nativeStockCountSave',
      'serviceCountCommand',
      'initialPhysicalStockCountWorkspace',
      'transitionInitialPhysicalStockCount',
    ]) {
      expect(src).toContain(retained);
    }
    const retained = RETAINED_TS_AUTHORITY[0]!;
    expect(read(retained.path.split(':')[0]!)).toContain(
      'export async function nativeStockCountTransition(',
    );
    // Deletion gate: these callers still depend on the retained helper, so the
    // shared handler and its three app re-exports cannot be deleted this round.
    for (const consumer of retained.consumers) {
      expect(read(consumer.split(':')[0]!)).toContain('nativeStockCountTransition');
    }
    expect(read(retained.testOracle.split(':')[0]!)).toContain('nativeStockCountTransition:nativeTransition');
    expect(retained.removalTaskIds).toEqual(['migrate.pos', 'migrate.manufacturing']);
  });
});
