import 'server-only';
import{createFinancePolicy}from '@erpx/finance-server/domain/finance-policy';
import{ApiError}from '@erpx/platform-runtime-server/public/http';
const { decimalToMinor } = createFinancePolicy((...args) => new ApiError(...args));
export{decimalToMinor};
