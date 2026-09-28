import {expect,it} from 'vitest';
import {productionPageCut,type PageCutEntry} from '../src/lib/hospitality-page-cut';

const base='/holdings/riverside/branches/hotel';
const catalogue:PageCutEntry[]=[
  {id:'crm',name:'CRM',state:'Coming Soon'},
  {id:'pms',name:'Hotel operations',state:'Coming Soon'},
  {id:'pos',name:'Restaurant point of sale',state:'Pilot'},
  {id:'support',name:'Support',state:'Coming Soon'},
  {id:'sales',name:'Sales',state:'Coming Soon'},
  {id:'procurement',name:'Purchasing',state:'Pilot'},
  {id:'inventory',name:'Inventory',state:'Coming Soon'},
  {id:'finance',name:'Finance',state:'Coming Soon'},
  {id:'operations',name:'Work',state:'Coming Soon'},
  {id:'people_payroll',name:'People',state:'Coming Soon'},
  {id:'identity_access',name:'Identity & access',state:'Coming Soon'},
  {id:'organization',name:'Organization',state:'Pilot'},
];
const cut=(path:string,query='')=>productionPageCut(`${base}/${path}`,base,new URLSearchParams(query),catalogue);

it('shows the effective production state on disabled module entry points',()=>{
  expect(cut('crm')).toMatchObject({id:'crm',state:'Coming Soon'});
  expect(cut('pms/front-desk')).toMatchObject({id:'pms',state:'Coming Soon'});
  expect(cut('support/sla')).toMatchObject({id:'support',state:'Coming Soon'});
  expect(cut('commerce','view=sales')).toMatchObject({id:'sales',state:'Coming Soon'});
  expect(cut('commerce')).toMatchObject({id:'sales',state:'Coming Soon'});
  expect(cut('commerce','view=unknown')).toMatchObject({id:'sales',state:'Coming Soon'});
  expect(cut('commerce','view=stock')).toMatchObject({id:'inventory',state:'Coming Soon'});
  expect(cut('commerce','view=parties&kind=customer')).toMatchObject({id:'crm',state:'Coming Soon'});
  expect(cut('commerce','view=parties&kind=supplier')).toBeNull();
  expect(cut('stock-counts')).toMatchObject({id:'inventory',state:'Coming Soon'});
  expect(cut('finance/bank')).toMatchObject({id:'finance',state:'Coming Soon'});
  expect(cut('staff-tasks')).toMatchObject({id:'operations',state:'Coming Soon'});
  expect(cut('people')).toMatchObject({id:'people_payroll',state:'Coming Soon'});
  expect(cut('people/organization')).toBeNull();
  expect(cut('people/settings','section=roles')).toMatchObject({id:'identity_access',state:'Coming Soon'});
  expect(cut('finance/engines')).toMatchObject({id:'advanced-finance',state:'Coming Soon'});
  expect(cut('people','view=payroll')).toMatchObject({id:'statutory-payroll',state:'Coming Soon'});
  expect(cut('pos')).toBeNull();
});

it('leaves authorized historical details and unrelated pilot screens readable',()=>{
  expect(cut('crm/lead-123')).toBeNull();
  expect(cut('pms/documents/invoice-123')).toBeNull();
  expect(cut('pms/folios','folio=folio-123')).toBeNull();
  expect(productionPageCut(`${base}/pos`,base,new URLSearchParams('sale=sale-123'),[{id:'pos',name:'Restaurant point of sale',state:'Coming Soon'}])).toBeNull();
  expect(cut('support/incidents/incident-123')).toBeNull();
  expect(cut('commerce','view=purchase')).toBeNull();
  expect(cut('people/payroll/run-123')).toBeNull();
  expect(cut('finance/invoices/invoice-123')).toBeNull();
  expect(cut('stock-counts/count-123')).toBeNull();
  expect(productionPageCut('/holdings/other/crm',base,new URLSearchParams(),catalogue)).toBeNull();
});
