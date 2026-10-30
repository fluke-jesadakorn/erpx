/** Only application pages can be used as a return destination, never external URLs or API actions. */
export function safeReturnPath(value: unknown): string | null {
    if (typeof value !== "string" || value.length > 2000 || /[\u0000-\u0020]/.test(value))
        return null;
    if (!/^\/(?:holdings\/|platform\/)/.test(value))
        return null;
    try {
        const url = new URL(value, "https://erpx.invalid");
        // Encoded slashes are ordinary filter values in queries and fragments.
        // Restrict route separators only in the path that controls navigation.
        if (value.split(/[?#]/, 1)[0].includes("\\") || /%(?:2f|5c|0[0-9a-f]|1[0-9a-f])/i.test(url.pathname))
            return null;
        return url.origin === "https://erpx.invalid" && /^\/(?:holdings\/[^/]+(?:\/.*)?|platform\/[^/]+(?:\/.*)?)$/.test(url.pathname) ? url.pathname + url.search + url.hash : null;
    }
    catch {
        return null;
    }
}
