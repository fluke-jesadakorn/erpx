import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
/** Only an explicitly isolated browser cohort can select this provider double. */
export function simulationBookingEmailProvider(env: Record<string, unknown>): typeof fetch | undefined {
    if (env.ERPX_ENVIRONMENT !== "local" || env.ERPX_EMAIL_ADAPTER !== "sandbox" || !["http://127.0.0.1:3102", "http://127.0.0.1:3104"].includes(String(env.ERPX_PUBLIC_ORIGIN)) || !/\/\.runtime\/browser\/[a-f0-9]{12}$/.test(String(env.ERPX_DATA_DIR)))
        return;
    return (async (request: string | URL | Request, init?: RequestInit) => {
        const url = typeof request === "string" || request instanceof URL ? String(request) : request.url;
        const key = new Headers(init?.headers).get("Idempotency-Key");
        if (url !== "https://api.resend.com/emails" || init?.method !== "POST" || !key?.startsWith("pms-booking-"))
            throw new Error("UNEXPECTED_SIMULATION_BOOKING_MAIL_REQUEST");
        return Response.json(canonicalDestinationLinks({ id: `sandbox-${key}`, adapter: "deterministic-booking-email-v1" }));
    }) as typeof fetch;
}
