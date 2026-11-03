import 'server-only';
import type {FinancePurchaseControls} from '@erpx/contracts/finance-purchase-controls';
import {NativeFinanceReadError} from './native-journal-detail';
const capabilities=['finance.accounting','finance.banking','finance.close','finance.advanced','finance.receivables','finance.tax','finance.service-invoice'];
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
export function nativeFinancePurchaseControls(value:unknown):FinancePurchaseControls {
 if(!object(value)||Object.keys(value).length!==capabilities.length||!capabilities.every(cap=>Object.hasOwn(value,cap)))throw new NativeFinanceReadError('NATIVE_FINANCE_RESPONSE_INVALID');
 for(const access of Object.values(value))if(!object(access)||Object.keys(access).sort().join(',')!=='history,newWork,revision,settleExisting'||typeof access.newWork!=='boolean'||typeof access.settleExisting!=='boolean'||typeof access.history!=='boolean'||typeof access.revision!=='string'||!access.revision)throw new NativeFinanceReadError('NATIVE_FINANCE_RESPONSE_INVALID');
 return value as FinancePurchaseControls;
}
