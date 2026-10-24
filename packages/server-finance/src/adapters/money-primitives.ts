import 'server-only';
import{createFinancePolicy}from '@erpx/server-finance/domain/finance-policy';
import{ApiError}from '@erpx/server-runtime/public/http';
const { decimalToMinor } = createFinancePolicy((...args) => new ApiError(...args));
export{decimalToMinor};
