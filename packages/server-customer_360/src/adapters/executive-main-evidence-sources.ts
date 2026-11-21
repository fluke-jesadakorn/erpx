import "server-only";
import { headers } from "next/headers";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { CustomerActivityItem } from "@erpx/server-customer_360/public/customer-timeline";
import {
    createNativeCustomerActivityClient,
    type NativeCustomerActivityError,
} from "./native-executive-customer-activity";

export type CustomerActivityTransport = Readonly<{
    search: (
        incoming: Pick<Headers, "get">,
        actor: WorkContext,
        input: {
            from: number;
            to: number;
            query: string;
            limit: number;
            cursorAt?: number;
            cursorId?: string;
        },
    ) => Promise<CustomerActivityItem[]>;
    read: (
        incoming: Pick<Headers, "get">,
        actor: WorkContext,
        recordId: string,
        filters: { from: number; to: number },
    ) => Promise<CustomerActivityItem | null>;
}>;

export type ExecutiveCustomerActivityOptions = Readonly<{
    incoming?: Pick<Headers, "get">;
    transport?: CustomerActivityTransport;
}>;

function nativeTransport(): CustomerActivityTransport {
    const raw = process.env.ERPX_NATIVE_CUSTOMER_ACTIVITY_CONFIG;
    if (raw === undefined)
        throw new ApiError(
            503,
            "NATIVE_CUSTOMER_ACTIVITY_CONFIGURATION_REQUIRED",
            "The native customer activity service is not configured.",
        );
    const client = createNativeCustomerActivityClient(raw);
    return {
        search: (incoming, actor, input) => client.search(incoming, actor, input),
        read: (incoming, actor, recordId, filters) =>
            client.read(incoming, actor, recordId, filters),
    };
}

function failure(error: unknown): never {
    if (error instanceof ApiError) throw error;
    const value = error as Partial<NativeCustomerActivityError>;
    throw new ApiError(
        value.status ?? 503,
        value.code ?? "NATIVE_CUSTOMER_ACTIVITY_UNAVAILABLE",
        "The native customer activity service is unavailable.",
    );
}

export function executiveMainEvidenceSources(
    db: SqlDatabase,
    options: ExecutiveCustomerActivityOptions = {},
) {
    void db;
    return {
        async factsSearchCustomerActivityEvidence(
            actor: WorkContext,
            input: {
                filters: { from: string; to: string; currency: string };
                query: string;
                limit: number;
                metric: "recent-customer-activity";
                cursor?: string | undefined;
            },
            cursor: { at: number; id: string } | null,
        ): Promise<{ rows: CustomerActivityItem[] }> {
            try {
                const incoming = options.incoming ?? (await headers());
                const transport = options.transport ?? nativeTransport();
                const rows = await transport.search(incoming, actor, {
                    from: Date.parse(input.filters.from),
                    to: Date.parse(input.filters.to) + 86400000,
                    query: input.query,
                    limit: input.limit,
                    cursorAt: cursor?.at,
                    cursorId: cursor?.id,
                });
                return { rows };
            } catch (error) {
                return failure(error);
            }
        },
        async factsReadCustomerActivityEvidence(
            actor: WorkContext,
            recordId: string,
            filters: { from: string; to: string; currency: string },
        ): Promise<{ row: CustomerActivityItem | null }> {
            try {
                const incoming = options.incoming ?? (await headers());
                const transport = options.transport ?? nativeTransport();
                const row = await transport.read(incoming, actor, recordId, {
                    from: Date.parse(filters.from),
                    to: Date.parse(filters.to) + 86400000,
                });
                return { row };
            } catch (error) {
                return failure(error);
            }
        },
    };
}
