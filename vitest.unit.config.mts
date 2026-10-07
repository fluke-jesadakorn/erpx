import { defineConfig } from "vitest/config";
import { vitestConfiguration } from "./scripts/testing/vitest-config";
export default defineConfig(vitestConfiguration("unit"));
