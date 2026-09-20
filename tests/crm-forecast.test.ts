import {it,expect} from 'vitest';
import {crmForecast,crmForecastDecimal} from '../src/lib/crm-forecast';
it('retains exact totals beyond safe Number aggregation and rounds each weighted opportunity half up',()=>{
 const rows=[{opportunity_id:'a',stage:'discovery',currency:'THB',expected_minor:'9007199254740991'},{opportunity_id:'b',stage:'negotiation',currency:'THB',expected_minor:'9007199254740991'},{opportunity_id:'c',stage:'proposal',currency:'USD',expected_minor:'1'},{opportunity_id:'d',stage:'lost',currency:'USD',expected_minor:'9000'},{opportunity_id:'e',stage:'won',currency:'USD',expected_minor:'123'}];
 const result=crmForecast(rows);
 expect(result.THB).toEqual({unweightedMinor:'18014398509481982',weightedMinor:'9007199254740991',wonMinor:'0'});
 expect(result.USD).toEqual({unweightedMinor:'1',weightedMinor:'1',wonMinor:'123'});
 expect(crmForecastDecimal(result.THB.unweightedMinor)).toBe('180143985094819.82');
});
