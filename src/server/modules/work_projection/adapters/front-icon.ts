import "server-only";
import sharp from "sharp";
import { createStorefrontIconService } from "../application/front-icon";
export const { storefrontAppIconPng } = createStorefrontIconService({base64:bytes=>Buffer.from(bytes).toString("base64"),png:async svg=>new Uint8Array(await sharp(Buffer.from(svg)).png().toBuffer())});
