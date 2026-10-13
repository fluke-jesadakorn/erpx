import type {BrowserContext} from '@playwright/test';

/** Paint-only privacy for shareable evidence; native text, inputs and MFA stay intact. */
export async function maskEngineeringSecurityEvidence(context:BrowserContext){
 await context.addInitScript(()=>{
  // Install before the document or React can paint an enrolled secret. A
  // constructable stylesheet changes no native elements, text or input values.
  const sheet=new CSSStyleSheet();
  sheet.replaceSync(`
   #security-content code, #security-content code *, #security-content pre,
   #security-content input[name="code"],
   #security-content input[autocomplete="one-time-code"] {
    color: transparent !important;
    -webkit-text-fill-color: transparent !important;
    text-shadow: none !important;
    caret-color: transparent !important;
    background-color: #183044 !important;
   }
   #security-content code::selection, #security-content pre::selection,
   #security-content input[name="code"]::selection,
   #security-content input[autocomplete="one-time-code"]::selection {
    color: transparent !important;
    -webkit-text-fill-color: transparent !important;
    text-shadow: none !important;
   }
   #security-content img, #security-content svg, #security-content canvas {
    opacity: 0 !important;
   }
  `);
  document.adoptedStyleSheets=[...document.adoptedStyleSheets,sheet];
 });
}
