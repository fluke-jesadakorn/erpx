import {registeredDatabases,type RegistryEnv} from "@erpx/feature-runtime/lib/holding-registry";
import {COMMERCE_SCHEMA_READY_SQL} from './read-tail-r15-dedupe-sql';
export async function billingTestFinanceReady(env:RegistryEnv){return registeredDatabases(env,'h003').write.prepare(COMMERCE_SCHEMA_READY_SQL('billing_test_finance_documents')).first<{ready:boolean}>();}
