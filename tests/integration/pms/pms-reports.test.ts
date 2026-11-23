import {describe,expect,it,vi} from 'vitest';
import {
  createNativePmsClient,
  nativePmsConfiguration,
  type NativePmsPageContext,
  type NativePmsReportData,
} from '../../../packages/feature-pms/src/adapters/server/native-pms';

// The TypeScript report SQL is retired. This suite now guards the native-only
// report boundary: the full owner payload (observation clock, pace, channels
// and Finance balances) is accepted, and a partial or malformed payload fails
// closed instead of falling back to a TypeScript projection.
const config=JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44994',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
});
const actor:NativePmsPageContext={
  userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:'branch',departmentId:null,assignmentId:'assignment',role:'pms_owner',
  assignmentScope:'branch',scope:{kind:'branch',branchSlug:'north'},
  authority:{revision:7,assignmentVersion:3},modules:['pms'],
  iam:{permissions:['pms.read','pms.reports','records.read']},
};
const context={
  userId:'user',holdingId:'h001',companyId:'company',branchId:'branch',
  departmentId:null,branchSlug:'north',departmentSlug:null,assignmentId:'assignment',
  assignmentVersion:3,revision:7,role:'pms_owner',assignmentScopeKind:'branch',
  modules:['pms'],permissions:['pms.read','pms.reports','records.read'],
  expiresAt:9_007_199_254_740_991,
};
const incoming=new Headers({host:'acme.example.com',cookie:'__Host-erpx-product=session'});
const ok=(data:unknown)=>new Response(JSON.stringify({data,context}),{status:200,headers:{'content-type':'application/json'}});
const range={
  property:{id:'property-1',name:'Riverside',branchId:'branch',state:'open',businessDate:'2026-11-01'},
  businessDate:'2026-11-01',currency:'THB',from:'2026-11-01',to:'2026-11-02',
  days:[{date:'2026-11-01',capacity:10,sold:4,roomRevenue:4000,netRevenue:4500,tax:200,refunds:0,cancellations:1,noShows:0}],
  totals:{capacity:10,sold:4,roomRevenue:4000,netRevenue:4500,tax:200,refunds:0,cancellations:1,noShows:0,occupancy:0.4,adr:1000,revpar:400},
  flow:{arrivals:2,departures:1,inHouse:4},
  balances:{source:'pms_guest_accounts',due:1000,advance:200,deposit:300,totalAccounts:3,openAccounts:2,creditAccounts:0,accountsWithDue:1},
  nightAudit:{businessDate:'2026-11-01',nightState:'completed',jobState:'done',ready:true,blockers:[]},
  asOf:'2026-11-03T10:00:00Z',
  pace:{
    current:{asOf:'2026-11-03T10:00:00Z',coverageSince:'2026-10-01T00:00:00Z',covered:true,reservations:9,roomNights:12,quotedTotal:120000},
    previous:{asOf:'2026-10-27T10:00:00Z',coverageSince:'2026-10-01T00:00:00Z',covered:true,reservations:7,roomNights:10,quotedTotal:100000},
    netPickup:{reservations:2,roomNights:2,quotedTotal:20000},
  },
  channels:[{source:'direct',reservations:4,roomNights:6,quotedTotal:60000,cancelled:1,noShows:0,recentPickup:2,unknownCreation:0}],
  financeBalances:{receivable:4000,advance:-3000,deposit:-500,refundPayable:-200,unallocated:0},
};
const selection={
  view:'range' as const,holdingSlug:'acme',property:'property-1',branchSlug:'north',
  from:'2026-11-01',to:'2026-11-02',
  asOf:'2026-11-03T10:00:00Z',compareAsOf:'2026-10-27T10:00:00Z',
};

describe('native-only PMS range report boundary',()=>{
  it('accepts the complete observation, pace, channel and Finance payload',async()=>{
    const transport=vi.fn().mockResolvedValue(ok(range));
    const client=createNativePmsClient(nativePmsConfiguration(config),transport);
    const result=await client.report(incoming,actor,selection) as NativePmsReportData;
    expect(result.asOf).toBe(selection.asOf);
    expect(result.pace.netPickup).toEqual({reservations:2,roomNights:2,quotedTotal:20000});
    expect(result.channels).toHaveLength(1);
    expect(result.financeBalances).toEqual(range.financeBalances);
    const url=new URL(transport.mock.calls[0][1]);
    expect(url.searchParams.get('asOf')).toBe(selection.asOf);
    expect(url.searchParams.get('compareAsOf')).toBe(selection.compareAsOf);
  });
  it('fails closed when any extended owner block is missing',async()=>{
    for(const key of ['asOf','pace','channels','financeBalances'] as const){
      const payload={...range};
      delete (payload as unknown as Record<string,unknown>)[key];
      const transport=vi.fn().mockResolvedValue(ok(payload));
      const client=createNativePmsClient(nativePmsConfiguration(config),transport);
      await expect(client.report(incoming,actor,selection)).rejects.toMatchObject({code:'NATIVE_PMS_RESPONSE_INVALID'});
    }
  });
  it('fails closed on a malformed observation time before dispatch',async()=>{
    const transport=vi.fn().mockResolvedValue(ok(range));
    const client=createNativePmsClient(nativePmsConfiguration(config),transport);
    await expect(client.report(incoming,actor,{...selection,asOf:'yesterday'})).rejects.toMatchObject({code:'NATIVE_PMS_REPORT_REQUEST_INVALID'});
    expect(transport).not.toHaveBeenCalled();
  });
});
