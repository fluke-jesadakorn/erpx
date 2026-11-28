import {registeredDatabases,type RegistryEnv} from "@erpx/platform-runtime-ui/lib/holding-registry";
import {nativeCommerceReadFirst,RD_BILLING_TEST_FINANCE_SCHEMA} from './native-commerce-read';
export async function billingTestFinanceReady(env:RegistryEnv){return nativeCommerceReadFirst<{ready:boolean}>(RD_BILLING_TEST_FINANCE_SCHEMA,[],registeredDatabases(env,'h003').write);}
