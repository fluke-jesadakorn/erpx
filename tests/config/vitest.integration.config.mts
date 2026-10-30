import { defineConfig } from "vitest/config";
import { vitestConfiguration } from "../../scripts/testing/vitest-config";
import { historicalServiceCountOracle } from "../support/field_service/legacy-count-oracle-plugin";
const config=vitestConfiguration("integration");
export default defineConfig({...config,plugins:[...(config.plugins??[]),historicalServiceCountOracle()]});
