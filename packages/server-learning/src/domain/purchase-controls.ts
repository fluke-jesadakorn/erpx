export type LearningCapability="learn.courses"|"learn.assignments"|"learn.results"|"learn.certificates"|"people.learner-reference";
export type LearningPurchaseControls=Partial<Record<LearningCapability,{newWork:boolean;settleExisting:boolean;history:boolean;revision:string}>>;
