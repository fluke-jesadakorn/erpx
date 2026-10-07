import "server-only";
import {createHash} from "node:crypto";
import sanitizeHtml from "sanitize-html";
import {createHtmlWidgetDocument} from "../../application/executive/html-widget";
const service=createHtmlWidgetDocument({sanitize:sanitizeHtml,allowedTags:sanitizeHtml.defaults.allowedTags,hash:value=>createHash("sha256").update(value).digest("hex"),documents:new Map()});
export const htmlWidgetDocument=service.htmlWidgetDocument;
