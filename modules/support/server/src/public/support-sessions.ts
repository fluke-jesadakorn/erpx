/** Type-only compatibility entry. Native Support session reads are mounted;
 * no runtime TypeScript session reader remains. */
import type {
    createSupportSessionDetail,
    createSupportSessionWorkspace,
} from "@erpx/support-server/application/support-sessions";

export type SupportSessionWorkspace = Awaited<ReturnType<ReturnType<typeof createSupportSessionWorkspace>>>;
export type SupportSessionDetail = Awaited<ReturnType<ReturnType<typeof createSupportSessionDetail>>>;
