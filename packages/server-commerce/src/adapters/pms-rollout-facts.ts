import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { nativeCommerceReadFirst } from "./native-commerce-read";

type Existence = Record<string, string | number | null>;

export function pmsRolloutCommerceFacts(db: SqlDatabase) {
    return {
        /**
         * Owner-local active-warehouse existence read. The fixed SQL is owned
         * by the native Commerce `command-plan` port; no TS SQL mirror remains.
         */
        inventory: (companyId: string) =>
            nativeCommerceReadFirst<Existence>('commerce_rd_pms_warehouse_active', [companyId], db),
        /**
         * Owner-local settings existence read, planned by the same native port.
         */
        commerce: (companyId: string) =>
            nativeCommerceReadFirst<Existence>('commerce_rd_pms_settings_exists', [companyId], db),
    };
}
