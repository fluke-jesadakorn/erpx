import {expect,it} from 'vitest';
import {productionPageCut,type PageCutEntry} from '../src/lib/hospitality-page-cut';

const base='/holdings/riverside/branches/hotel';
const catalogue:PageCutEntry[]=[
  {id:'crm',name:'CRM',state:'Coming Soon'},
  {id:'pms',name:'Hotel operations',state:'Coming Soon'},
  {id:'pos',name:'Restaurant point of sale',state:'Pilot'},
  {id:'support',name:'Support',state:'Coming Soon'},
];
const cut=(path:string,query='')=>productionPageCut(`${base}/${path}`,base,new URLSearchParams(query),catalogue);

it('shows the effective production state on disabled module entry points',()=>{
  expect(cut('crm')).toMatchObject({id:'crm',state:'Coming Soon'});
  expect(cut('pms/front-desk')).toMatchObject({id:'pms',state:'Coming Soon'});
  expect(cut('support/sla')).toMatchObject({id:'support',state:'Coming Soon'});
  expect(cut('commerce','view=sales')).toMatchObject({id:'sales',state:'Coming Soon'});
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
  expect(productionPageCut('/holdings/other/crm',base,new URLSearchParams(),catalogue)).toBeNull();
});
