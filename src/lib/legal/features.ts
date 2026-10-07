import {parseFeature as parse,validatePrivacyCase as validate} from "./features-domain";
export {featureSpecs,featureKinds,featureInput,privacyFieldChoices,workflowFieldChoices,privacyDecisionError} from "./features-domain";
export type {FeatureField,FeatureSpec,FeatureKind,FeatureRow} from "./features-domain";
export function parseFeature(raw:unknown){return parse(raw,Date.now());}
export function validatePrivacyCase(data:Record<string,string>){return validate(data,Date.now());}
