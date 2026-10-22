import type {LearningPurchaseControls,LearningCapability} from "@erpx/server-learning/public/purchases";
export type {LearningPurchaseControls,LearningCapability};
export function learningPurchased(purchases:LearningPurchaseControls|undefined,capability:LearningCapability,purpose:'newWork'|'settleExisting'|'history'){return !purchases||purchases[capability]?.[purpose]===true;}
export function learningResultPurchased(purchases:LearningPurchaseControls|undefined,requiredFor:unknown){return learningPurchased(purchases,'learn.assignments','settleExisting')&&learningPurchased(purchases,'learn.results','settleExisting')&&(!requiredFor||learningPurchased(purchases,'learn.certificates','settleExisting'));}
type Row=Record<string,string|number|null>;
export type LearningConsoleData={courses:Row[];people:Row[];canWrite:boolean;canReview:boolean;actorId:string;purchases?:LearningPurchaseControls};
export type LearningConsoleDetail=Omit<LearningConsoleData,'courses'|'people'>&{assignment:Row};
/** Only fields used by these controls cross the client boundary. Other
 * learners' assignments, results, histories and certificates remain server-side. */
export function learningConsoleSnapshot(source:LearningConsoleData&{assignment?:Row},detail:true):LearningConsoleDetail;
export function learningConsoleSnapshot(source:LearningConsoleData&{assignment?:Row},detail?:false):LearningConsoleData;
export function learningConsoleSnapshot(source:LearningConsoleData&{assignment?:Row},detail=false):LearningConsoleData|LearningConsoleDetail{
 const common={actorId:source.actorId,canWrite:source.canWrite,canReview:source.canReview,...(source.purchases?{purchases:source.purchases}:{})};
 if(detail)return {...common,assignment:Object.fromEntries(['id','version','status','learner_id','learner_eligible','required_for'].map(key=>[key,source.assignment?.[key]??null]))};
 return {...common,courses:source.courses.map(course=>Object.fromEntries(['id','version','title','status','created_by'].map(key=>[key,course[key]??null]))),people:source.canReview&&learningPurchased(source.purchases,"learn.assignments","newWork")?source.people.map(person=>({id:person.id,name:person.name})):[]};
}
