import { defineConfig } from "vitest/config";
import { vitestConfiguration } from "../../scripts/testing/vitest-config";
// Compatibility entry point; package scripts use separate unit/integration runners.
export default defineConfig(vitestConfiguration("all"));
