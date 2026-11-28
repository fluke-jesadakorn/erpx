const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
export function encodeBase32(bytes: Uint8Array) {
    let bits = 0, value = 0, out = "";
    for (const byte of bytes) {
        value = (value << 8) | byte;
        bits += 8;
        while (bits >= 5) {
            out += alphabet[(value >>> (bits - 5)) & 31];
            bits -= 5;
        }
    }
    if (bits)
        out += alphabet[(value << (5 - bits)) & 31];
    return out;
}
export const authenticatorCodeValid = (code: string) => /^\d{6}$/.test(code);
export const authenticatorCounters = (now: number) => [now - 1, now, now + 1];
