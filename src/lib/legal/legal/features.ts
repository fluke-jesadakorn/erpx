import {parseFeature as parse,validatePrivacyCase as validate} from "@/lib/legal/legal/features-domain";
export {featureSpecs,featureKinds,featureInput,privacyFieldChoices,workflowFieldChoices,privacyDecisionError} from "@/lib/legal/legal/features-domain";
export type {FeatureField,FeatureSpec,FeatureKind,FeatureRow} from "@/lib/legal/legal/features-domain";
export function parseFeature(raw:unknown){return parse(raw,Date.now());}
export function validatePrivacyCase(data:Record<string,string>){return validate(data,Date.now());}
