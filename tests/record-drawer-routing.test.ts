import {expect,it} from 'vitest';
import {recordTarget,recordDrawerHref,recordCanonicalHref,withoutRecordDrawer} from '../src/lib/record-drawer';
const base='/holdings/acme';
it('keeps the register filters, cursor and fragment when selecting and closing a record',()=>{
 const current=`${base}/commerce?view=purchase&q=desk&cursor=page2#orders`;
 const selected=recordDrawerHref(current,{kind:'order',id:'po:line / encoded',args:{view:'purchase'}});
 expect(withoutRecordDrawer(selected)).toBe(current);
 const q=new URL(selected,'https://erpx.invalid').searchParams;
 expect(q.get('detailId')).toBe('po:line / encoded');
 expect(q.get('detailArgs')).toBe('{"view":"purchase"}');
});
it('preserves the source dimensions in full-page and modified-click destinations',()=>{
 const target={kind:'stock_position' as const,id:'product:1',args:{warehouseId:'warehouse:2'}};
 expect(recordTarget(recordCanonicalHref(base,target),base)).toEqual(target);
});
it('keeps print, external origins and other work scopes on their original routes',()=>{
 expect(recordTarget(`${base}/finance/invoices/1?print=1`,base)).toBeNull();
 expect(recordTarget('https://example.com/holdings/acme/finance/invoices/1',base)).toBeNull();
 expect(recordTarget('/holdings/another/finance/invoices/1',base)).toBeNull();
});
it('does not mistake module list and creation routes for record IDs',()=>{
 for(const path of ['people/organization','people/payroll','support/incidents','support/sla','people/move','maintenance/plans','requests/new'])expect(recordTarget(`${base}/${path}`,base)).toBeNull();
});
it('resolves Legal ID queries and encoded owner IDs without changing their identity',()=>{
 expect(recordTarget(`${base}/legal?kind=contract&record=a%3Ab`,base)?.id).toBe('a:b');
 expect(recordTarget(`${base}/finance/journals/maintenance%3Aspare`,base)?.id).toBe('maintenance:spare');
});
