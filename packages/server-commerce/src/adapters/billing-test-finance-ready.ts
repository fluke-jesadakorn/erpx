import {registeredDatabases,type RegistryEnv} from "@erpx/feature-runtime/lib/holding-registry";
export async function billingTestFinanceReady(env:RegistryEnv){return registeredDatabases(env,'h003').write.prepare("SELECT to_regclass('billing_test_finance_documents') IS NOT NULL ready").first<{ready:boolean}>();}
