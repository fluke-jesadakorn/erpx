import {readFile,readdir} from 'node:fs/promises';
import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
vi.mock('next/headers',()=>({headers:async()=>new Headers({host:'acme.example.com',cookie:'__Host-erpx-product=session'})}));
vi.mock('@erpx/talent-server/public/purchases',()=>({talentPurchaseControls:vi.fn(async()=>undefined)}));
import {
  createNativeTalentClient,
  nativeTalentAvailable,
  nativeTalentConfiguration,
  NativeTalentError,
  type NativeTalentPageContext,
} from '../../../modules/talent/ui/src/public/native-talent-requisition';
import {
  nativeTalentFailure,
  nativeTalentRequisitionCommand,
  nativeTalentWorkspace,
} from '../../../modules/talent/ui/src/public/native-talent-page';
import {nativeTalentLifecycleCommand} from '../../../modules/talent/server/src/public/native-talent-lifecycle-page';
import {nativeTalentPlan,nativeTalentReadAll,nativeTalentReadFirst} from '../../../modules/talent/server/src/adapters/native-talent-read';
import {talentOwnerSourceImage,talentOwnerSourceWitnessAssertion} from '../../../modules/talent/server/src/adapters/purchases';
import {SensitiveAttachmentTalentFacts} from '../../../modules/talent/server/src/adapters/attachments-sensitive-facts';
import {peopleHireHandoffFacts} from '../../../modules/talent/server/src/adapters/people-hire-handoff-facts';
import {talentPurchaseControls} from '../../../modules/talent/server/src/public/purchases';

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44993',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  ...overrides,
});
const incoming=()=>new Headers({
  host:'acme.example.com',
  cookie:'__Host-erpx-product=session',
});
const actor:NativeTalentPageContext={
  userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:'branch',departmentId:null,assignmentId:'assignment',role:'people_manager',
  assignmentScope:'branch',scope:{kind:'branch',branchSlug:'north'},
  authority:{revision:7,assignmentVersion:3},modules:['talent'],
  iam:{permissions:['records.read','records.write','records.review']},
};
const context={userId:'user',holdingId:'h001',companyId:'company',branchId:'branch',
  departmentId:null,branchSlug:'north',departmentSlug:null,assignmentId:'assignment',
  assignmentVersion:3,revision:7,role:'people_manager',assignmentScopeKind:'branch',
  modules:['talent'],permissions:['records.read','records.write','records.review'],
  expiresAt:9_007_199_254_740_991};
const row={id:'talent-req-1',company_id:'company',branch_id:'branch',department_id:null,
  job_title:'Service specialist',openings:2,target_start_date:'2026-11-01',
  description:'Serve customer cases and complete field handoffs.',status:'draft',
  created_by:'creator',reviewed_by:null,version:1,created_at:1_799_999_999_000,
  updated_at:1_799_999_999_000,canApprove:true,canReject:true,canOpen:false,
  canClose:false,canCancel:true};
const event={version:1,action:'create',actor_id:'creator',reason:'Hiring need drafted',
  status:'draft',created_at:1_799_999_999_000};
const input={holdingSlug:'acme',branchSlug:'north',
  expected:{companyId:'company',assignmentId:'assignment',revision:7,assignmentVersion:3}};
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});

describe('native Talent requisition transport',()=>{
 it('forwards the exact workspace Host, session and context CAS',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:{requisitions:[row]},context}));
  const talent=createNativeTalentClient(nativeTalentConfiguration(config()),transport);
  const result=await talent.workspace(incoming(),actor,input);
  expect(result.requisitions).toEqual([row]);
  const [method,url,headers,body]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/talent/requisitions');
  expect(headers.get('host')).toBe('acme.example.com');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment')).toBe('assignment');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
  expect(headers.get('x-erpx-workspace-branch')).toBe('north');
  expect(headers.get('idempotency-key')).toBeNull();
  expect(body).toBeUndefined();
 });

 it('forwards the exact requisition detail path and history',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:{requisition:row,events:[event]},context}));
  const talent=createNativeTalentClient(nativeTalentConfiguration(config()),transport);
  const result=await talent.detail(incoming(),actor,'talent-req-1',input);
  expect(result.events).toEqual([event]);
  const [method,url]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/talent/requisitions/talent-req-1');
 });

 it('forwards the exact create and change bodies with one idempotency key',async()=>{
  const transport=vi.fn()
    .mockResolvedValueOnce(ok({data:{id:'talent-req-1',status:'draft',version:1},context}))
    .mockResolvedValueOnce(ok({data:{id:'talent-req-1',status:'approved',version:2},context}));
  const talent=createNativeTalentClient(nativeTalentConfiguration(config()),transport);
  const create={jobTitle:'Service specialist',openings:2,targetStartDate:'2026-11-01',
    description:'Serve customer cases and complete field handoffs.'};
  expect(await talent.create(incoming(),actor,JSON.stringify(create),'talent-requisition-command-1',input))
    .toEqual({id:'talent-req-1',status:'draft',version:1});
  const [,createUrl,createHeaders,createBody]=transport.mock.calls[0];
  expect(createUrl.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/talent/requisitions');
  expect(createHeaders.get('idempotency-key')).toBe('talent-requisition-command-1');
  expect(createHeaders.get('content-type')).toBe('application/json');
  expect(JSON.parse(createBody!)).toEqual(create);
  const change={action:'approve',expectedVersion:1,reason:'Headcount and scope reviewed'};
  expect(await talent.change(incoming(),actor,'talent-req-1',JSON.stringify(change),'talent-requisition-command-2',input))
    .toEqual({id:'talent-req-1',status:'approved',version:2});
  const [changeMethod,changeUrl,changeHeaders,changeBody]=transport.mock.calls[1];
  expect(changeMethod).toBe('POST');
  expect(changeUrl.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/talent/requisitions/talent-req-1');
  expect(changeHeaders.get('idempotency-key')).toBe('talent-requisition-command-2');
  expect(JSON.parse(changeBody!)).toEqual(change);
 });

 it('surfaces a bounded owner error with its status, code and message',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(
    JSON.stringify({error:{code:'CONFLICT',message:'Reload the current requisition decision.'}}),
    {status:409,headers:{'content-type':'application/json'}},
  ));
  const talent=createNativeTalentClient(nativeTalentConfiguration(config()),transport);
  await expect(talent.change(incoming(),actor,'talent-req-1','{}','talent-requisition-command-3',input))
    .rejects.toMatchObject({code:'CONFLICT',status:409,message:'Reload the current requisition decision.'});
 });

 it('rejects foreign scope, duplicate rows, extra keys and changed context',async()=>{
  const foreign=createNativeTalentClient(nativeTalentConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{requisitions:[{...row,company_id:'foreign'}]},context}),
  ));
  await expect(foreign.workspace(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_TALENT_RESPONSE_INVALID'});
  const duplicate=createNativeTalentClient(nativeTalentConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{requisitions:[row,row]},context}),
  ));
  await expect(duplicate.workspace(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_TALENT_RESPONSE_INVALID'});
  const extra=createNativeTalentClient(nativeTalentConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{requisition:{...row,extra:'field'},events:[event]},context}),
  ));
  await expect(extra.detail(incoming(),actor,'talent-req-1',input))
    .rejects.toMatchObject({code:'NATIVE_TALENT_RESPONSE_INVALID'});
  const changed=createNativeTalentClient(nativeTalentConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{requisitions:[]},context:{...context,revision:8}}),
  ));
  await expect(changed.workspace(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_TALENT_CONTEXT_CHANGED'});
  const unknownHost=createNativeTalentClient(nativeTalentConfiguration(config()),vi.fn());
  await expect(unknownHost.workspace(new Headers({host:'evil.example.com'}),actor,input))
    .rejects.toMatchObject({code:'NATIVE_TALENT_AUTHORITY_INVALID'});
 });

 it('fails closed when the native Talent configuration is absent',async()=>{
  const previous=process.env.ERPX_NATIVE_TALENT_CONFIG;
  delete process.env.ERPX_NATIVE_TALENT_CONFIG;
  expect(nativeTalentAvailable()).toBe(false);
  expect(await nativeTalentWorkspace(actor as never)).toBeUndefined();
  expect(await nativeTalentRequisitionCommand(
    actor as never,
    {entity:'requisition',jobTitle:'Service specialist',openings:2,targetStartDate:'2026-11-01',description:'Serve customer cases.'},
    'talent-requisition-command-9',
  )).toBeUndefined();
  if(previous===undefined)delete process.env.ERPX_NATIVE_TALENT_CONFIG;else process.env.ERPX_NATIVE_TALENT_CONFIG=previous;
 });

 it('retains non-requisition commands and rejects ineligible requisition input when configured',async()=>{
  const previous=process.env.ERPX_NATIVE_TALENT_CONFIG;
  process.env.ERPX_NATIVE_TALENT_CONFIG=config();
  expect(await nativeTalentRequisitionCommand(
    actor as never,
    {entity:'application',id:'talent-application-1',action:'interview',expectedVersion:1,reason:'Schedule interview'},
    'talent-requisition-command-10',
  )).toBeUndefined();
  await expect(nativeTalentRequisitionCommand(
    actor as never,
    {entity:'requisition',id:'talent-req-1',action:'send',expectedVersion:1,reason:'Invalid action'},
    'talent-requisition-command-11',
  )).rejects.toMatchObject({status:422,code:'TALENT_REQUISITION_INVALID'});
  if(previous===undefined)delete process.env.ERPX_NATIVE_TALENT_CONFIG;else process.env.ERPX_NATIVE_TALENT_CONFIG=previous;
 });

 it('enforces the retained recruit.positions purchase preflight before a configured command',async()=>{
  const previous=process.env.ERPX_NATIVE_TALENT_CONFIG;
  process.env.ERPX_NATIVE_TALENT_CONFIG=config();
  vi.mocked(talentPurchaseControls).mockResolvedValueOnce({
    'recruit.positions':{newWork:false,settleExisting:true,history:true,revision:'revision'},
  } as never);
  await expect(nativeTalentRequisitionCommand(
    actor as never,
    {entity:'requisition',jobTitle:'Service specialist',openings:2,targetStartDate:'2026-11-01',description:'Serve customer cases.'},
    'talent-requisition-command-12',
  )).rejects.toMatchObject({status:403,code:'TALENT_PURCHASE_REQUIRED'});
  if(previous===undefined)delete process.env.ERPX_NATIVE_TALENT_CONFIG;else process.env.ERPX_NATIVE_TALENT_CONFIG=previous;
 });

 it('enforces the recruit lifecycle purchase preflight before a configured candidate create',async()=>{
  const previous=process.env.ERPX_NATIVE_TALENT_CONFIG;
  process.env.ERPX_NATIVE_TALENT_CONFIG=config();
  vi.mocked(talentPurchaseControls).mockResolvedValueOnce({
    'recruit.candidates':{newWork:false,settleExisting:true,history:true,revision:'revision'},
  } as never);
  await expect(nativeTalentLifecycleCommand(
    new Request('https://acme.example.com/api/v1/talent',{method:'POST',headers:incoming()}),
    actor as never,
    'acme',
    {entity:'candidate',legalName:'Candidate',email:'candidate@example.test',consentEvidence:'Consent',retentionUntil:'2027-01-01'},
    'talent-lifecycle-command-1',
  )).rejects.toMatchObject({status:403,code:'TALENT_PURCHASE_REQUIRED'});
  if(previous===undefined)delete process.env.ERPX_NATIVE_TALENT_CONFIG;else process.env.ERPX_NATIVE_TALENT_CONFIG=previous;
 });

 it('maps a malformed configured selector to a bounded 503 without fallback',async()=>{
  const previous=process.env.ERPX_NATIVE_TALENT_CONFIG;
  process.env.ERPX_NATIVE_TALENT_CONFIG='{not-json';
  await expect(nativeTalentWorkspace(actor as never)).rejects.toMatchObject({
    status:503,code:'NATIVE_TALENT_UNAVAILABLE',
  });
  if(previous===undefined)delete process.env.ERPX_NATIVE_TALENT_CONFIG;else process.env.ERPX_NATIVE_TALENT_CONFIG=previous;
 });

 it('rejects reserved loopback ports, insecure Holding origins and malformed configuration',()=>{
  expect(()=>createNativeTalentClient(nativeTalentConfiguration(config({privateBaseUrl:'http://127.0.0.1:3100'})),vi.fn())).toThrow();
  expect(()=>createNativeTalentClient(nativeTalentConfiguration(config({privateBaseUrl:'http://10.0.0.1:44993'})),vi.fn())).toThrow();
  expect(()=>createNativeTalentClient(nativeTalentConfiguration(config({
    holdings:[{origin:'http://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  })),vi.fn())).toThrow();
  expect(()=>nativeTalentConfiguration('{not-json')).toThrow();
 });

 it('maps transport failures to the retained page/route status policy',()=>{
  expect(nativeTalentFailure(new NativeTalentError('NATIVE_TALENT_CONTEXT_CHANGED')))
    .toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});
  expect(nativeTalentFailure(new NativeTalentError('NATIVE_TALENT_REQUEST_INVALID')))
    .toMatchObject({status:422,code:'TALENT_REQUISITION_INVALID'});
  expect(nativeTalentFailure(new NativeTalentError('NATIVE_TALENT_AUTHORITY_INVALID')))
    .toMatchObject({status:503,code:'NATIVE_TALENT_UNAVAILABLE'});
  expect(nativeTalentFailure(new NativeTalentError('CONFLICT',409,'Reload.')))
    .toMatchObject({status:409,code:'CONFLICT'});
  expect(nativeTalentFailure(new Error('other'))).toBeUndefined();
 });
});

describe('native Talent detail read tail', () => {
  const readConfig = JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44994',
    timeoutMs: 1000,
    holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
  });

  it('plans owner detail reads through the mounted command-plan route with no local SQL', async () => {
    const previous = process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
    process.env.ERPX_NATIVE_TALENT_READ_CONFIG = readConfig;
    const plannedSql = 'SELECT * FROM talent_offers WHERE application_id=?';
    const fetchMock = vi.fn(async (_url: unknown, init: { body?: string }) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      expect(body.kind).toBe('talent_rd_offers');
      expect(body.values).toEqual(['app-1']);
      return ok({ data: { statements: [{ sql: plannedSql, values: ['app-1'] }] } });
    });
    vi.stubGlobal('fetch', fetchMock);
    const seen: string[] = [];
    const db = {
      prepare(sql: string) {
        seen.push(sql);
        return {
          bind(...values: unknown[]) {
            expect(values).toEqual(['app-1']);
            return { first: async () => ({ id: 'offer-1' }), all: async () => ({ results: [{ id: 'offer-1' }] }) };
          },
        };
      },
    } as never;
    try {
      await expect(nativeTalentReadFirst('talent_rd_offers', ['app-1'], db)).resolves.toEqual({ id: 'offer-1' });
      expect(seen).toEqual([plannedSql]);
      expect(String(fetchMock.mock.calls[0][0])).toContain('/api/v1/holdings/acme/talent/command-plan');
    } finally {
      vi.unstubAllGlobals();
      if (previous === undefined) delete process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
      else process.env.ERPX_NATIVE_TALENT_READ_CONFIG = previous;
    }
  });

  it('plans the candidate privacy-history read through the mounted command-plan route', async () => {
    const previous = process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
    process.env.ERPX_NATIVE_TALENT_READ_CONFIG = readConfig;
    const plannedSql = 'SELECT id,action,candidate_version,created_at FROM talent_candidate_privacy_events WHERE candidate_id=? AND company_id=? ORDER BY candidate_version,id';
    const fetchMock = vi.fn(async (_url: unknown, init: { body?: string }) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      expect(body.kind).toBe('talent_rd_privacy_history');
      expect(body.values).toEqual(['cand-1', 'company']);
      return ok({ data: { statements: [{ sql: plannedSql, values: body.values }] } });
    });
    vi.stubGlobal('fetch', fetchMock);
    const seen: string[] = [];
    const db = {
      prepare(sql: string) {
        seen.push(sql);
        return {
          bind(...values: unknown[]) {
            expect(values).toEqual(['cand-1', 'company']);
            return { all: async () => ({ results: [{ id: 'e1', action: 'hold', candidate_version: 1, created_at: 1 }] }) };
          },
        };
      },
    } as never;
    try {
      const rows = await nativeTalentReadAll<{ id: string; action: string }>('talent_rd_privacy_history', ['cand-1', 'company'], db);
      expect(rows).toEqual([{ id: 'e1', action: 'hold', candidate_version: 1, created_at: 1 }]);
      expect(seen).toEqual([plannedSql]);
      expect(String(fetchMock.mock.calls[0][0])).toContain('/api/v1/holdings/acme/talent/command-plan');
    } finally {
      vi.unstubAllGlobals();
      if (previous === undefined) delete process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
      else process.env.ERPX_NATIVE_TALENT_READ_CONFIG = previous;
    }
  });

  it('plans the workspace list reads through the mounted command-plan route', async () => {
    const previous = process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
    process.env.ERPX_NATIVE_TALENT_READ_CONFIG = readConfig;
    const cases: Array<{ kind: string; values: unknown[]; sql: string }> = [
      {
        kind: 'talent_rd_workspace_requisitions',
        values: ['company', 'branch', 'branch', null, null],
        sql: 'SELECT * FROM talent_requisitions WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) ORDER BY updated_at DESC',
      },
      {
        kind: 'talent_rd_workspace_candidates',
        values: ['company'],
        sql: 'SELECT id,legal_name,email,phone,retention_until,status,version,updated_at FROM talent_candidates WHERE company_id=? ORDER BY updated_at DESC',
      },
    ];
    const fetchMock = vi.fn(async (_url: unknown, init: { body?: string }) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      const match = cases.find((item) => item.kind === body.kind);
      expect(match).toBeDefined();
      expect(body.values).toEqual(match!.values);
      return ok({ data: { statements: [{ sql: match!.sql, values: body.values }] } });
    });
    vi.stubGlobal('fetch', fetchMock);
    const seen: string[] = [];
    const db = {
      prepare(sql: string) {
        seen.push(sql);
        return {
          bind(...values: unknown[]) {
            return { all: async () => ({ results: values }) };
          },
        };
      },
    } as never;
    try {
      for (const item of cases) {
        const rows = await nativeTalentReadAll<unknown>(item.kind, item.values, db);
        expect(rows).toEqual(item.values);
      }
      expect(seen).toEqual([cases[0].sql, cases[1].sql]);
      expect(String(fetchMock.mock.calls[0][0])).toContain('/api/v1/holdings/acme/talent/command-plan');
      expect(fetchMock).toHaveBeenCalledTimes(2);
    } finally {
      vi.unstubAllGlobals();
      if (previous === undefined) delete process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
      else process.env.ERPX_NATIVE_TALENT_READ_CONFIG = previous;
    }
  });

  it('plans the sensitive offer read through the mounted command-plan route', async () => {
    const previous = process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
    process.env.ERPX_NATIVE_TALENT_READ_CONFIG = readConfig;
    const plannedSql = 'SELECT o.company_id,o.version,r.branch_id,r.department_id,a.owner_id,CASE WHEN (o.status<>\'accepted\' OR EXISTS(SELECT 1 FROM talent_hire_handoffs handoff JOIN talent_hire_consumption_receipts receipt ON receipt.handoff_id=handoff.id AND receipt.company_id=handoff.company_id AND receipt.source_offer_version=handoff.source_offer_version WHERE handoff.offer_id=o.id AND handoff.company_id=o.company_id)) THEN 0 ELSE 1 END pending_hire_delivery FROM talent_offers o JOIN talent_applications a ON a.id=o.application_id AND a.company_id=o.company_id JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id WHERE o.id=? AND o.company_id=?';
    const fetchMock = vi.fn(async (_url: unknown, init: { body?: string }) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      expect(body.kind).toBe('talent_rd_sensitive_offer');
      expect(body.values).toEqual(['offer-1', 'company']);
      return ok({ data: { statements: [{ sql: plannedSql, values: body.values }] } });
    });
    vi.stubGlobal('fetch', fetchMock);
    const seen: string[] = [];
    const db = {
      prepare(sql: string) {
        seen.push(sql);
        return {
          bind(...values: unknown[]) {
            expect(values).toEqual(['offer-1', 'company']);
            return { first: async () => ({ company_id: 'company', pending_hire_delivery: 0 }) };
          },
        };
      },
    } as never;
    try {
      await expect(SensitiveAttachmentTalentFacts(db).readTalentOffers1({ targetId: 'offer-1', companyId: 'company' }))
        .resolves.toMatchObject({ company_id: 'company', pending_hire_delivery: 0 });
      expect(seen).toEqual([plannedSql]);
      expect(String(fetchMock.mock.calls[0][0])).toContain('/api/v1/holdings/acme/talent/command-plan');
      expect(fetchMock).toHaveBeenCalledTimes(1);
    } finally {
      vi.unstubAllGlobals();
      if (previous === undefined) delete process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
      else process.env.ERPX_NATIVE_TALENT_READ_CONFIG = previous;
    }
  });

  it('plans the sensitive application read through the mounted command-plan route', async () => {
    const previous = process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
    process.env.ERPX_NATIVE_TALENT_READ_CONFIG = readConfig;
    const plannedSql = 'SELECT c.company_id,c.version,min(r.branch_id) FILTER (WHERE a.owner_id=?) owner_branch_id,min(r.department_id) FILTER (WHERE a.owner_id=?) owner_department_id,CASE WHEN EXISTS(SELECT 1 FROM talent_applications owned WHERE owned.candidate_id=c.id AND owned.company_id=c.company_id AND owned.owner_id=?) THEN 1 ELSE 0 END is_owner FROM talent_candidates c LEFT JOIN talent_applications a ON a.candidate_id=c.id AND a.company_id=c.company_id LEFT JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id WHERE c.id=? AND c.company_id=? GROUP BY c.id,c.company_id,c.version';
    const values = ['user', 'user', 'user', 'cand-1', 'company'];
    const fetchMock = vi.fn(async (_url: unknown, init: { body?: string }) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      expect(body.kind).toBe('talent_rd_sensitive_applications');
      expect(body.values).toEqual(values);
      return ok({ data: { statements: [{ sql: plannedSql, values: body.values }] } });
    });
    vi.stubGlobal('fetch', fetchMock);
    const seen: string[] = [];
    const db = {
      prepare(sql: string) {
        seen.push(sql);
        return {
          bind(...bound: unknown[]) {
            expect(bound).toEqual(values);
            return { first: async () => ({ company_id: 'company', owner_branch_id: 'branch', is_owner: 1 }) };
          },
        };
      },
    } as never;
    try {
      await expect(SensitiveAttachmentTalentFacts(db).readTalentApplications1({
        userId: 'user', userId2: 'user', userId3: 'user', targetId: 'cand-1', companyId: 'company',
      })).resolves.toMatchObject({ company_id: 'company', owner_branch_id: 'branch', is_owner: 1 });
      expect(seen).toEqual([plannedSql]);
      expect(String(fetchMock.mock.calls[0][0])).toContain('/api/v1/holdings/acme/talent/command-plan');
      expect(fetchMock).toHaveBeenCalledTimes(1);
    } finally {
      vi.unstubAllGlobals();
      if (previous === undefined) delete process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
      else process.env.ERPX_NATIVE_TALENT_READ_CONFIG = previous;
    }
  });

  it('plans the hire handoff fact reads through the mounted command-plan route', async () => {
    const previous = process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
    process.env.ERPX_NATIVE_TALENT_READ_CONFIG = readConfig;
    const sourceSql = 'SELECT h.*,o.version offer_version,o.status offer_status,o.start_date,o.currency,o.base_amount_minor,o.accepted_evidence,r.job_title,c.version candidate_version,c.status candidate_status,c.retention_until,c.legal_name,c.email,c.phone,c.privacy_hold,c.consent_evidence FROM talent_hire_handoffs h JOIN talent_offers o ON o.id=h.offer_id AND o.company_id=h.company_id JOIN talent_applications a ON a.id=o.application_id AND a.company_id=h.company_id JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=h.company_id AND r.branch_id=h.branch_id AND r.department_id IS NOT DISTINCT FROM h.department_id JOIN talent_candidates c ON c.id=h.candidate_id AND c.id=a.candidate_id AND c.company_id=h.company_id WHERE h.id=? AND h.company_id=?';
    const queueSql = 'SELECT h.*,o.application_id,o.currency,o.base_amount_minor,o.start_date,o.version offer_version,c.legal_name,c.version candidate_version,c.status candidate_status,r.job_title,x.id receipt_id,x.person_id,x.engagement_id FROM talent_hire_handoffs h JOIN talent_offers o ON o.id=h.offer_id JOIN talent_candidates c ON c.id=h.candidate_id JOIN talent_applications a ON a.id=o.application_id JOIN talent_requisitions r ON r.id=a.requisition_id LEFT JOIN talent_hire_consumption_receipts x ON x.handoff_id=h.id WHERE h.company_id=? ORDER BY h.created_at,h.id';
    const cases: Array<{ kind: string; values: unknown[]; sql: string }> = [
      { kind: 'talent_rd_hire_source_row', values: ['handoff-1', 'company'], sql: sourceSql },
      { kind: 'talent_rd_hire_handoff_queue', values: ['company'], sql: queueSql },
    ];
    const fetchMock = vi.fn(async (_url: unknown, init: { body?: string }) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      const match = cases.find((item) => item.kind === body.kind);
      expect(match).toBeDefined();
      expect(body.values).toEqual(match!.values);
      return ok({ data: { statements: [{ sql: match!.sql, values: body.values }] } });
    });
    vi.stubGlobal('fetch', fetchMock);
    const sourceRow = { id: 'handoff-1', offer_status: 'accepted', candidate_version: 2 };
    const queueRows = [{ id: 'handoff-1', receipt_id: 'receipt-1' }];
    const seen: string[] = [];
    const db = {
      prepare(sql: string) {
        seen.push(sql);
        return {
          bind(...values: unknown[]) {
            if (values.length === 2) return { first: async () => sourceRow };
            return { all: async () => ({ results: queueRows }) };
          },
        };
      },
    } as never;
    try {
      const facts = peopleHireHandoffFacts(db);
      await expect(facts.readHireSourceRow('handoff-1', 'company')).resolves.toMatchObject(sourceRow);
      await expect(facts.readHireHandoffQueueEffect('company')).resolves.toEqual({ results: queueRows });
      expect(seen).toEqual([sourceSql, queueSql]);
      expect(String(fetchMock.mock.calls[0][0])).toContain('/api/v1/holdings/acme/talent/command-plan');
      expect(fetchMock).toHaveBeenCalledTimes(2);
    } finally {
      vi.unstubAllGlobals();
      if (previous === undefined) delete process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
      else process.env.ERPX_NATIVE_TALENT_READ_CONFIG = previous;
    }
  });

  it('fails closed without a native read config and never builds local SQL', async () => {
    const previous = process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
    delete process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
    const db = { prepare: () => { throw new Error('owner-local reader must not build local SQL'); } } as never;
    try {
      await expect(nativeTalentReadAll('talent_rd_offers', ['app-1'], db))
        .rejects.toMatchObject({ status: 503, code: 'NATIVE_TALENT_READ_UNAVAILABLE' });
    } finally {
      if (previous !== undefined) process.env.ERPX_NATIVE_TALENT_READ_CONFIG = previous;
    }
  });
});

// G4 / migrate.talent.offer-read-port: the offer/candidate/requisition
// caller-unit read plans registered by the native port and proved byte-identical
// by the live-PG parity suite. Exact SQL is pinned so no TS literal returns.
const offerFamily = [
  {
    kind: 'talent_rd_candidate_source_image',
    constant: 'RD_CANDIDATE_SOURCE_IMAGE',
    sql: 'SELECT to_jsonb(c)::text image FROM talent_candidates c WHERE c.id=? AND c.company_id=?',
    binds: 2,
  },
  {
    kind: 'talent_rd_candidate_source_image_file',
    constant: 'RD_CANDIDATE_SOURCE_IMAGE_FILE',
    sql: "SELECT (to_jsonb(c)-'version'-'updated_at')::text image FROM talent_candidates c WHERE c.id=? AND c.company_id=?",
    binds: 2,
  },
  {
    kind: 'talent_rd_candidate_events',
    constant: 'RD_CANDIDATE_EVENTS',
    sql: 'SELECT * FROM talent_candidate_events WHERE candidate_id=? ORDER BY version',
    binds: 1,
  },
  {
    kind: 'talent_rd_offer_events',
    constant: 'RD_OFFER_EVENTS',
    sql: 'SELECT * FROM talent_offer_events WHERE offer_id=? ORDER BY version',
    binds: 1,
  },
  {
    kind: 'talent_rd_requisition_source_image',
    constant: 'RD_REQUISITION_SOURCE_IMAGE',
    sql: 'SELECT to_jsonb(r)::text image FROM talent_requisitions r WHERE r.id=? AND r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?)',
    binds: 6,
  },
  {
    kind: 'talent_rd_requisition_source_image_file',
    constant: 'RD_REQUISITION_SOURCE_IMAGE_FILE',
    sql: "SELECT (to_jsonb(r)-'version'-'updated_at')::text image FROM talent_requisitions r WHERE r.id=? AND r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?)",
    binds: 6,
  },
  {
    kind: 'talent_rd_offer_source_image',
    constant: 'RD_OFFER_SOURCE_IMAGE',
    sql: "SELECT jsonb_build_object('record',to_jsonb(t),'application',to_jsonb(a),'requisition',to_jsonb(r),'candidate',to_jsonb(c),'ownerEligible',EXISTS(SELECT 1 FROM assignments owner_assignment WHERE owner_assignment.company_id=a.company_id AND owner_assignment.user_id=a.owner_id AND iam_assignment_active(owner_assignment.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(owner_assignment.user_id,owner_assignment.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (owner_assignment.scope_kind='holding' OR owner_assignment.scope_kind='branch' AND owner_assignment.scope_node_id=r.branch_id OR owner_assignment.scope_kind='department' AND owner_assignment.scope_node_id=r.department_id)))::text image FROM talent_offers t JOIN talent_applications a ON a.id=t.application_id AND a.company_id=t.company_id JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id  WHERE t.id=? AND r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?) AND t.application_version<=a.version",
    binds: 6,
  },
  {
    kind: 'talent_rd_offer_source_image_file',
    constant: 'RD_OFFER_SOURCE_IMAGE_FILE',
    sql: "SELECT jsonb_build_object('record',to_jsonb(t)-'version'-'updated_at','application',to_jsonb(a),'requisition',to_jsonb(r),'candidate',to_jsonb(c),'ownerEligible',EXISTS(SELECT 1 FROM assignments owner_assignment WHERE owner_assignment.company_id=a.company_id AND owner_assignment.user_id=a.owner_id AND iam_assignment_active(owner_assignment.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(owner_assignment.user_id,owner_assignment.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (owner_assignment.scope_kind='holding' OR owner_assignment.scope_kind='branch' AND owner_assignment.scope_node_id=r.branch_id OR owner_assignment.scope_kind='department' AND owner_assignment.scope_node_id=r.department_id)))::text image FROM talent_offers t JOIN talent_applications a ON a.id=t.application_id AND a.company_id=t.company_id JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id  WHERE t.id=? AND r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?) AND t.application_version<=a.version",
    binds: 6,
  },
  {
    kind: 'talent_rd_handoff_source_image',
    constant: 'RD_HANDOFF_SOURCE_IMAGE',
    sql: "SELECT jsonb_build_object('record',to_jsonb(t),'application',to_jsonb(a),'requisition',to_jsonb(r),'candidate',to_jsonb(c),'ownerEligible',EXISTS(SELECT 1 FROM assignments owner_assignment WHERE owner_assignment.company_id=a.company_id AND owner_assignment.user_id=a.owner_id AND iam_assignment_active(owner_assignment.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(owner_assignment.user_id,owner_assignment.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (owner_assignment.scope_kind='holding' OR owner_assignment.scope_kind='branch' AND owner_assignment.scope_node_id=r.branch_id OR owner_assignment.scope_kind='department' AND owner_assignment.scope_node_id=r.department_id)),'offer',to_jsonb(o),'receipt',to_jsonb(receipt),'people',hired.image)::text image FROM talent_hire_handoffs t JOIN talent_offers o ON o.id=t.offer_id AND o.company_id=t.company_id JOIN talent_applications a ON a.id=o.application_id AND a.company_id=o.company_id JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id LEFT JOIN talent_hire_consumption_receipts receipt ON receipt.handoff_id=t.id AND receipt.company_id=t.company_id LEFT JOIN (SELECT receipt.company_id,receipt.handoff_id,jsonb_build_object('person',to_jsonb(person),'engagement',to_jsonb(engagement),'compensation',to_jsonb(compensation),'payGroup',to_jsonb(pay_group)) image FROM talent_hire_consumption_receipts receipt JOIN hr_people person ON person.id=receipt.person_id AND person.company_id=receipt.company_id JOIN hr_engagements engagement ON engagement.id=receipt.engagement_id AND engagement.person_id=person.id AND engagement.company_id=receipt.company_id JOIN hr_compensation_revisions compensation ON compensation.id=receipt.compensation_id AND compensation.engagement_id=engagement.id JOIN hr_pay_groups pay_group ON pay_group.id=engagement.pay_group_id AND pay_group.company_id=receipt.company_id) hired ON hired.handoff_id=t.id AND hired.company_id=t.company_id WHERE t.id=? AND r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?) AND t.candidate_id=c.id AND t.branch_id=r.branch_id AND t.department_id IS NOT DISTINCT FROM r.department_id AND t.source_offer_version<=o.version",
    binds: 6,
  },
  {
    kind: 'talent_rd_handoff_source_image_file',
    constant: 'RD_HANDOFF_SOURCE_IMAGE_FILE',
    sql: "SELECT jsonb_build_object('record',to_jsonb(t)-'version'-'updated_at','application',to_jsonb(a),'requisition',to_jsonb(r),'candidate',to_jsonb(c),'ownerEligible',EXISTS(SELECT 1 FROM assignments owner_assignment WHERE owner_assignment.company_id=a.company_id AND owner_assignment.user_id=a.owner_id AND iam_assignment_active(owner_assignment.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(owner_assignment.user_id,owner_assignment.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (owner_assignment.scope_kind='holding' OR owner_assignment.scope_kind='branch' AND owner_assignment.scope_node_id=r.branch_id OR owner_assignment.scope_kind='department' AND owner_assignment.scope_node_id=r.department_id)),'offer',to_jsonb(o),'receipt',to_jsonb(receipt),'people',hired.image)::text image FROM talent_hire_handoffs t JOIN talent_offers o ON o.id=t.offer_id AND o.company_id=t.company_id JOIN talent_applications a ON a.id=o.application_id AND a.company_id=o.company_id JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id LEFT JOIN talent_hire_consumption_receipts receipt ON receipt.handoff_id=t.id AND receipt.company_id=t.company_id LEFT JOIN (SELECT receipt.company_id,receipt.handoff_id,jsonb_build_object('person',to_jsonb(person),'engagement',to_jsonb(engagement),'compensation',to_jsonb(compensation),'payGroup',to_jsonb(pay_group)) image FROM talent_hire_consumption_receipts receipt JOIN hr_people person ON person.id=receipt.person_id AND person.company_id=receipt.company_id JOIN hr_engagements engagement ON engagement.id=receipt.engagement_id AND engagement.person_id=person.id AND engagement.company_id=receipt.company_id JOIN hr_compensation_revisions compensation ON compensation.id=receipt.compensation_id AND compensation.engagement_id=engagement.id JOIN hr_pay_groups pay_group ON pay_group.id=engagement.pay_group_id AND pay_group.company_id=receipt.company_id) hired ON hired.handoff_id=t.id AND hired.company_id=t.company_id WHERE t.id=? AND r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?) AND t.candidate_id=c.id AND t.branch_id=r.branch_id AND t.department_id IS NOT DISTINCT FROM r.department_id AND t.source_offer_version<=o.version",
    binds: 6,
  },
  {
    kind: 'talent_rd_application_source_image',
    constant: 'RD_APPLICATION_SOURCE_IMAGE',
    sql: "SELECT jsonb_build_object('record',to_jsonb(t),'application',to_jsonb(t),'requisition',to_jsonb(r),'candidate',to_jsonb(c),'ownerEligible',EXISTS(SELECT 1 FROM assignments owner_assignment WHERE owner_assignment.company_id=t.company_id AND owner_assignment.user_id=t.owner_id AND iam_assignment_active(owner_assignment.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(owner_assignment.user_id,owner_assignment.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (owner_assignment.scope_kind='holding' OR owner_assignment.scope_kind='branch' AND owner_assignment.scope_node_id=r.branch_id OR owner_assignment.scope_kind='department' AND owner_assignment.scope_node_id=r.department_id)))::text image FROM talent_applications t JOIN talent_requisitions r ON r.id=t.requisition_id AND r.company_id=t.company_id JOIN talent_candidates c ON c.id=t.candidate_id AND c.company_id=t.company_id  WHERE t.id=? AND r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?) AND t.requisition_version<=r.version",
    binds: 6,
  },
  {
    kind: 'talent_rd_application_source_image_file',
    constant: 'RD_APPLICATION_SOURCE_IMAGE_FILE',
    sql: "SELECT jsonb_build_object('record',to_jsonb(t)-'version'-'updated_at','application',to_jsonb(t),'requisition',to_jsonb(r),'candidate',to_jsonb(c),'ownerEligible',EXISTS(SELECT 1 FROM assignments owner_assignment WHERE owner_assignment.company_id=t.company_id AND owner_assignment.user_id=t.owner_id AND iam_assignment_active(owner_assignment.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(owner_assignment.user_id,owner_assignment.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (owner_assignment.scope_kind='holding' OR owner_assignment.scope_kind='branch' AND owner_assignment.scope_node_id=r.branch_id OR owner_assignment.scope_kind='department' AND owner_assignment.scope_node_id=r.department_id)))::text image FROM talent_applications t JOIN talent_requisitions r ON r.id=t.requisition_id AND r.company_id=t.company_id JOIN talent_candidates c ON c.id=t.candidate_id AND c.company_id=t.company_id  WHERE t.id=? AND r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?) AND t.requisition_version<=r.version",
    binds: 6,
  },
  {
    kind: 'talent_rd_proposal_source_image',
    constant: 'RD_PROPOSAL_SOURCE_IMAGE',
    sql: "SELECT jsonb_build_object('record',to_jsonb(t),'application',to_jsonb(a),'requisition',to_jsonb(r),'candidate',to_jsonb(c),'ownerEligible',EXISTS(SELECT 1 FROM assignments owner_assignment WHERE owner_assignment.company_id=a.company_id AND owner_assignment.user_id=a.owner_id AND iam_assignment_active(owner_assignment.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(owner_assignment.user_id,owner_assignment.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (owner_assignment.scope_kind='holding' OR owner_assignment.scope_kind='branch' AND owner_assignment.scope_node_id=r.branch_id OR owner_assignment.scope_kind='department' AND owner_assignment.scope_node_id=r.department_id)))::text image FROM talent_proposals t JOIN talent_applications a ON a.id=t.application_id JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id  WHERE t.id=? AND r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?) AND t.application_version<=a.version",
    binds: 6,
  },
  {
    kind: 'talent_rd_proposal_source_image_file',
    constant: 'RD_PROPOSAL_SOURCE_IMAGE_FILE',
    sql: "SELECT jsonb_build_object('record',to_jsonb(t)-'version'-'updated_at','application',to_jsonb(a),'requisition',to_jsonb(r),'candidate',to_jsonb(c),'ownerEligible',EXISTS(SELECT 1 FROM assignments owner_assignment WHERE owner_assignment.company_id=a.company_id AND owner_assignment.user_id=a.owner_id AND iam_assignment_active(owner_assignment.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(owner_assignment.user_id,owner_assignment.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (owner_assignment.scope_kind='holding' OR owner_assignment.scope_kind='branch' AND owner_assignment.scope_node_id=r.branch_id OR owner_assignment.scope_kind='department' AND owner_assignment.scope_node_id=r.department_id)))::text image FROM talent_proposals t JOIN talent_applications a ON a.id=t.application_id JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id  WHERE t.id=? AND r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?) AND t.application_version<=a.version",
    binds: 6,
  },
] as const;

const adapter = new URL('../../../modules/talent/server/src/adapters/', import.meta.url);
const src = (name: string) => readFile(new URL(name, adapter), 'utf8');
async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('talent offer/candidate native read port cutover', () => {
  it('registers every family kind as an owner-authored SELECT-only plan', async () => {
    const registry = await src('native_talent_read_sql.rs');
    for (const read of offerFamily) {
      expect(registry, read.kind).toContain(`pub const ${read.constant}: &str = "${read.kind}"`);
      expect(registry, read.kind).toContain(`"${read.sql}"`);
      expect(registry, read.kind).toContain(`READ_${read.constant.slice(3)},`);
    }
    for (const forbidden of ['INSERT INTO', 'UPDATE ', 'DELETE FROM', 'RETURNING', 'FOR UPDATE', ';']) {
      expect(offerFamily.some((read) => read.sql.includes(forbidden)), forbidden).toBe(false);
    }
  });

  it('keeps the offer and candidate detail reads on the native client only', async () => {
    const [talent, sensitive, bridge] = await Promise.all([
      src('talent.ts'), src('attachments-sensitive-facts.ts'), src('native-talent-read.ts'),
    ]);
    expect(talent).not.toContain("nativeTalentReadFirst<Row>('talent_rd_offers',[id],await database())");
    expect(talent).not.toContain("nativeTalentReadFirst<Row>('talent_rd_hire_handoffs',[offerId],await database())");
    expect(talent).toContain('NATIVE_TALENT_DETAIL_UNAVAILABLE');
    expect(sensitive).toContain("'talent_rd_sensitive_offer'");
    expect(sensitive).toContain('[facts.targetId,facts.companyId],db)');
    expect(bridge).not.toMatch(/SELECT\s/);
    expect(bridge).not.toMatch(/INSERT\s|UPDATE\s|DELETE\s/);
    expect(bridge).toContain('NATIVE_TALENT_READ_UNAVAILABLE');
    expect(bridge).toContain('return unavailable()');
  });

  it('sends only kind and positional values for all fourteen native reads', async () => {
    const previous = process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
    process.env.ERPX_NATIVE_TALENT_READ_CONFIG = config();
    const calls: { kind: string; values: unknown[] }[] = [];
    const fetchMock = vi.fn(async (_url: unknown, init: { body?: string }) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      calls.push(body);
      const read = offerFamily.find((item) => item.kind === body.kind);
      expect(read, body.kind).toBeDefined();
      expect(body.values).toHaveLength(read!.binds);
      return ok({ data: { statements: [{ sql: read!.sql, values: body.values }] } });
    });
    vi.stubGlobal('fetch', fetchMock);
    try {
      for (const read of offerFamily) {
        const values = Array.from({ length: read.binds }, (_, index) => `v${index}`);
        const seen: string[] = [];
        const db = {
          prepare(sql: string) {
            seen.push(sql);
            return {
              bind(...bound: unknown[]) {
                expect(bound).toEqual(values);
                return { first: async () => ({ image: 'row' }), all: async () => ({ results: [{ image: 'row' }] }) };
              },
            };
          },
        } as never;
        if (read.kind.endsWith('_events')) {
          expect(await nativeTalentReadAll<{ image: string }>(read.kind, values, db)).toEqual([{ image: 'row' }]);
        } else {
          expect(await nativeTalentReadFirst<{ image: string }>(read.kind, values, db)).toEqual({ image: 'row' });
        }
        expect(seen).toEqual([read.sql]);
      }
      expect(calls.map((call) => call.kind)).toEqual(offerFamily.map((read) => read.kind));
      expect(String(fetchMock.mock.calls[0][0])).toContain('/api/v1/holdings/acme/talent/command-plan');
    } finally {
      vi.unstubAllGlobals();
      if (previous === undefined) delete process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
      else process.env.ERPX_NATIVE_TALENT_READ_CONFIG = previous;
    }
  });

  it('pins the owner SELECT KPI at the post-detail-cutover baseline of 9', async () => {
    let count = 0;
    const srcDir = new URL('../../../modules/talent/server/src/', import.meta.url);
    for (const file of await tsFiles(srcDir)) count += (await readFile(file, 'utf8')).split('SELECT ').length - 1;
    expect(count).toBe(9);
    expect(count).toBeLessThan(12);
  });

  it('reads candidate source images from the native port with no local SQL', async () => {
    const previous = process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
    process.env.ERPX_NATIVE_TALENT_READ_CONFIG = config();
    const kinds: string[] = [];
    const fetchMock = vi.fn(async (_url: unknown, init: { body?: string }) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      kinds.push(body.kind);
      expect(body.values).toEqual(['cand-1', 'h001']);
      const read = offerFamily.find((item) => item.kind === body.kind)!;
      return ok({ data: { statements: [{ sql: read.sql, values: body.values }] } });
    });
    vi.stubGlobal('fetch', fetchMock);
    try {
      for (const file of [false, true]) {
        const seen: string[] = [];
        const db = {
          prepare(sql: string) { seen.push(sql); return { bind() { return { first: async () => ({ image: 'image-json' }) }; } }; },
        } as never;
        const actor = { companyId: 'h001' } as never;
        expect(await talentOwnerSourceImage(db, actor, { kind: 'candidate', id: 'cand-1' }, file)).toBe('image-json');
        expect(seen).toEqual([file ? offerFamily[1].sql : offerFamily[0].sql]);
      }
      expect(kinds).toEqual(['talent_rd_candidate_source_image', 'talent_rd_candidate_source_image_file']);
    } finally {
      vi.unstubAllGlobals();
      if (previous === undefined) delete process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
      else process.env.ERPX_NATIVE_TALENT_READ_CONFIG = previous;
    }
  });

  it('builds the deferred candidate witness from the owner-planned statement', async () => {
    const previous = process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
    process.env.ERPX_NATIVE_TALENT_READ_CONFIG = config();
    const planned = offerFamily[0].sql;
    const fetchMock = vi.fn(async (_url: unknown, init: { body?: string }) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      expect(body).toEqual({ kind: 'talent_rd_candidate_source_image', values: ['cand-1', 'h001'] });
      return ok({ data: { statements: [{ sql: planned, values: body.values }] } });
    });
    vi.stubGlobal('fetch', fetchMock);
    const db = { prepare: () => { throw new Error('witness must not author local SQL'); } } as never;
    try {
      const descriptor = await talentOwnerSourceWitnessAssertion(
        db, 'key-1', 'talent-owner-source', { companyId: 'h001' } as never,
        { kind: 'candidate', id: 'cand-1' }, 'witness-json',
      );
      expect((descriptor as { kind: string }).kind).toBe('wf_native_command_assertions_3');
      expect((descriptor as unknown as { values: unknown[] }).values).toEqual([
        'key-1', 'talent-owner-source', 'cand-1', 'h001', 'witness-json', `(${planned})=?`,
      ]);
      expect(fetchMock).toHaveBeenCalledTimes(1);
      // Plan-only helper exposes the same owner statement with no local SQL.
      expect(await nativeTalentPlan('talent_rd_candidate_source_image', ['cand-1', 'h001'])).toEqual({
        sql: planned, values: ['cand-1', 'h001'],
      });
    } finally {
      vi.unstubAllGlobals();
      if (previous === undefined) delete process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
      else process.env.ERPX_NATIVE_TALENT_READ_CONFIG = previous;
    }
  });

  it('reads requisition source images from the native port with no local SQL', async () => {
    const previous = process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
    process.env.ERPX_NATIVE_TALENT_READ_CONFIG = config();
    const values = ['req-1', 'h001', 'b-1', 'b-1', null, null];
    const kinds: string[] = [];
    const fetchMock = vi.fn(async (_url: unknown, init: { body?: string }) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      kinds.push(body.kind);
      expect(body.values).toEqual(values);
      const read = offerFamily.find((item) => item.kind === body.kind)!;
      return ok({ data: { statements: [{ sql: read.sql, values: body.values }] } });
    });
    vi.stubGlobal('fetch', fetchMock);
    try {
      for (const file of [false, true]) {
        const seen: string[] = [];
        const db = {
          prepare(sql: string) {
            seen.push(sql);
            return { bind(...bound: unknown[]) { expect(bound).toEqual(values); return { first: async () => ({ image: 'req-image' }) }; } };
          },
        } as never;
        const actor = { companyId: 'h001', branchId: 'b-1', departmentId: null } as never;
        expect(await talentOwnerSourceImage(db, actor, { kind: 'requisition', id: 'req-1' }, file)).toBe('req-image');
        expect(seen).toEqual([file ? offerFamily[5].sql : offerFamily[4].sql]);
      }
      expect(kinds).toEqual(['talent_rd_requisition_source_image', 'talent_rd_requisition_source_image_file']);
    } finally {
      vi.unstubAllGlobals();
      if (previous === undefined) delete process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
      else process.env.ERPX_NATIVE_TALENT_READ_CONFIG = previous;
    }
  });

  it('builds the deferred requisition witness from the owner-planned statement', async () => {
    const previous = process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
    process.env.ERPX_NATIVE_TALENT_READ_CONFIG = config();
    const planned = offerFamily[4].sql;
    const values = ['req-1', 'h001', 'b-1', 'b-1', null, null];
    const fetchMock = vi.fn(async (_url: unknown, init: { body?: string }) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      expect(body).toEqual({ kind: 'talent_rd_requisition_source_image', values });
      return ok({ data: { statements: [{ sql: planned, values: body.values }] } });
    });
    vi.stubGlobal('fetch', fetchMock);
    const db = { prepare: () => { throw new Error('witness must not author local SQL'); } } as never;
    try {
      const descriptor = await talentOwnerSourceWitnessAssertion(
        db, 'key-2', 'talent-owner-source',
        { companyId: 'h001', branchId: 'b-1', departmentId: null } as never,
        { kind: 'requisition', id: 'req-1' }, 'req-witness-json',
      );
      expect((descriptor as { kind: string }).kind).toBe('wf_native_command_assertions_3');
      expect((descriptor as unknown as { values: unknown[] }).values).toEqual([
        'key-2', 'talent-owner-source', ...values, 'req-witness-json', `(${planned})=?`,
      ]);
      expect(fetchMock).toHaveBeenCalledTimes(1);
    } finally {
      vi.unstubAllGlobals();
      if (previous === undefined) delete process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
      else process.env.ERPX_NATIVE_TALENT_READ_CONFIG = previous;
    }
  });

  it('reads offer and hire-handoff composites and witnesses from native plans', async () => {
    const previous = process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
    process.env.ERPX_NATIVE_TALENT_READ_CONFIG = config();
    const values = ['source-1', 'h001', 'b-1', 'b-1', null, null];
    const actor = { companyId: 'h001', branchId: 'b-1', departmentId: null } as never;
    const calls: string[] = [];
    const fetchMock = vi.fn(async (_url: unknown, init: { body?: string }) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      calls.push(body.kind);
      expect(body.values).toEqual(values);
      const read = offerFamily.find((item) => item.kind === body.kind)!;
      return ok({ data: { statements: [{ sql: read.sql, values: body.values }] } });
    });
    vi.stubGlobal('fetch', fetchMock);
    try {
      for (const kind of ['offer', 'handoff'] as const) {
        for (const file of [false, true]) {
          const expected = offerFamily.find((read) => read.kind === `talent_rd_${kind}_source_image${file ? '_file' : ''}`)!;
          const seen: string[] = [];
          const db = {
            prepare(sql: string) {
              seen.push(sql);
              return { bind(...bound: unknown[]) { expect(bound).toEqual(values); return { first: async () => ({ image: 'composite' }) }; } };
            },
          } as never;
          expect(await talentOwnerSourceImage(db, actor, { kind, id: 'source-1' }, file)).toBe('composite');
          expect(seen).toEqual([expected.sql]);
          const spyDb = { prepare: () => { throw new Error('witness must not author local SQL'); } } as never;
          const descriptor = await talentOwnerSourceWitnessAssertion(
            spyDb, `key-${kind}-${file}`, 'talent-owner-source', actor, { kind, id: 'source-1' }, 'witness', file,
          );
          expect((descriptor as unknown as { values: unknown[] }).values).toEqual([
            `key-${kind}-${file}`, 'talent-owner-source', ...values, 'witness', `(${expected.sql})=?`,
          ]);
        }
      }
      expect(calls.filter((kind) => kind.includes('_source_image'))).toHaveLength(8);
    } finally {
      vi.unstubAllGlobals();
      if (previous === undefined) delete process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
      else process.env.ERPX_NATIVE_TALENT_READ_CONFIG = previous;
    }
  });

  it('reads application and proposal composites and witnesses from native plans', async () => {
    const previous = process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
    process.env.ERPX_NATIVE_TALENT_READ_CONFIG = config();
    const values = ['source-1', 'h001', 'b-1', 'b-1', null, null];
    const actor = { companyId: 'h001', branchId: 'b-1', departmentId: null } as never;
    const fetchMock = vi.fn(async (_url: unknown, init: { body?: string }) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      expect(body.values).toEqual(values);
      const read = offerFamily.find((item) => item.kind === body.kind)!;
      return ok({ data: { statements: [{ sql: read.sql, values: body.values }] } });
    });
    vi.stubGlobal('fetch', fetchMock);
    try {
      for (const kind of ['application', 'proposal'] as const) {
        for (const file of [false, true]) {
          const expected = offerFamily.find((read) => read.kind === `talent_rd_${kind}_source_image${file ? '_file' : ''}`)!;
          const seen: string[] = [];
          const db = {
            prepare(sql: string) {
              seen.push(sql);
              return { bind(...bound: unknown[]) { expect(bound).toEqual(values); return { first: async () => ({ image: 'composite' }) }; } };
            },
          } as never;
          expect(await talentOwnerSourceImage(db, actor, { kind, id: 'source-1' }, file)).toBe('composite');
          expect(seen).toEqual([expected.sql]);
          const descriptor = await talentOwnerSourceWitnessAssertion(
            { prepare: () => { throw new Error('witness must not author local SQL'); } } as never,
            `key-${kind}-${file}`, 'talent-owner-source', actor, { kind, id: 'source-1' }, 'witness', file,
          );
          expect((descriptor as unknown as { values: unknown[] }).values).toEqual([
            `key-${kind}-${file}`, 'talent-owner-source', ...values, 'witness', `(${expected.sql})=?`,
          ]);
        }
      }
      expect(fetchMock).toHaveBeenCalledTimes(8);
    } finally {
      vi.unstubAllGlobals();
      if (previous === undefined) delete process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
      else process.env.ERPX_NATIVE_TALENT_READ_CONFIG = previous;
    }
  });
});

describe('native-only Talent application detail GET cutover', () => {
  const routeUrl = new URL('../../../products/recruit/ui/src/routes/api/v1/holdings/[holdingSlug]/talent/route.ts', import.meta.url);
  const readerUrl = new URL('../../../modules/talent/server/src/adapters/talent.ts', import.meta.url);
  const nativeUrl = new URL('../../../modules/talent/server/src/adapters/native_talent_detail/http.rs', import.meta.url);
  const sqlUrl = new URL('../../../modules/talent/server/src/adapters/native_talent_detail/postgres.rs', import.meta.url);

  it('deletes the shared detail GET and keeps the write POST', async () => {
    const ts = await import('typescript');
    const source = await readFile(routeUrl, 'utf8');
    const tree = ts.createSourceFile('actual-talent-route.ts', source, ts.ScriptTarget.Latest, true);
    const exported = tree.statements.filter(ts.isFunctionDeclaration)
      .filter((fn) => fn.modifiers?.some((modifier) => modifier.kind === ts.SyntaxKind.ExportKeyword))
      .map((fn) => fn.name?.text);
    expect(exported).toEqual(['POST']);
    for (const symbol of ['talentDetail', 'talentWorkspace', 'nativeTalentWorkspace', 'searchParams.get("id")']) expect(source).not.toContain(symbol);
  });

  it('leaves the detail GET to the mounted owner route with real SQL execution', async () => {
    const http = await readFile(nativeUrl, 'utf8');
    const pg = await readFile(sqlUrl, 'utf8');
    expect(http).toContain('/api/v1/holdings/{holding_slug}/talent/applications/{application_id}/detail');
    expect(http).toContain('get(read_detail)');
    for (const statement of ['DETAIL_APPLICATION_SQL', 'DETAIL_HISTORY_SQL', 'DETAIL_PROPOSAL_SQL', 'DETAIL_OFFER_SQL', 'DETAIL_HANDOFF_SQL']) expect(pg).toContain(statement);
    expect(pg).toMatch(/sqlx::query_scalar/);
    expect(pg).not.toContain('fetch_all');
  });

  it('fails closed on the compatibility detail reader with no TS SQL', async () => {
    const ts = await import('typescript');
    const source = await readFile(readerUrl, 'utf8');
    expect(source).not.toContain('nativeTalentDetail');
    expect(source).not.toContain('createTalentDetail(');
    const tree = ts.createSourceFile('actual-talent-adapter.ts', source, ts.ScriptTarget.Latest, true);
    const fn = tree.statements.find((node) => ts.isFunctionDeclaration(node) && node.name?.text === 'talentDetail');
    expect(fn).toBeDefined();
    const compiled = ts.transpileModule(fn!.getText(tree).replace(/^export\s+/, ''), { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS } }).outputText;
    const { ApiError } = await import('@erpx/platform-runtime-server/public/http');
    const read = new Function('ApiError', `${compiled}\nreturn talentDetail;`)(ApiError) as (actor: unknown, id: string) => Promise<unknown>;
    for (const id of ['app-1', 'app-missing']) await expect(read({ companyId: 'company' }, id)).rejects.toMatchObject({ status: 503, code: 'NATIVE_TALENT_DETAIL_UNAVAILABLE' });
  });
});
