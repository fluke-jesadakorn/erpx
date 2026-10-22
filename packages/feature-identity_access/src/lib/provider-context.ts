export type ProviderContextSeed={accountKey:string;contextKey:string;selectedId:string;revision:number;choices:{id:string;label:string;version:string}[]};
export type ProviderContextSelection={accountKey:string;expectedContext:string;expectedRevision:number;assignmentId:string;choiceVersion:string};
const opaque=(value:unknown):value is string=>typeof value==='string'&&/^[a-f0-9]{64}$/.test(value);
export function providerContextSeed(value:unknown):ProviderContextSeed{
 const data=value as ProviderContextSeed;
 if(!data||!opaque(data.accountKey)||!opaque(data.contextKey)||typeof data.selectedId!=='string'||!Number.isSafeInteger(data.revision)||data.revision<0||!Array.isArray(data.choices)||!data.choices.length||data.choices.some(choice=>!choice||typeof choice.id!=='string'||!choice.id||typeof choice.label!=='string'||!opaque(choice.version))||new Set(data.choices.map(choice=>choice.id)).size!==data.choices.length||!data.choices.some(choice=>choice.id===data.selectedId))throw Error('Provider context could not be verified. Reload Administration.');
 return {accountKey:data.accountKey,contextKey:data.contextKey,selectedId:data.selectedId,revision:data.revision,choices:data.choices.map(({id,label,version})=>({id,label,version}))};
}
export function providerContextSelection(seed:ProviderContextSeed,assignmentId:string):ProviderContextSelection{
 const choice=seed.choices.find(row=>row.id===assignmentId);if(!choice)throw Error('This Platform role is unavailable.');
 return {accountKey:seed.accountKey,expectedContext:seed.contextKey,expectedRevision:seed.revision,assignmentId,choiceVersion:choice.version};
}
export function providerContextAcknowledgement(value:unknown,input:ProviderContextSelection):ProviderContextSeed{
 const seed=providerContextSeed(value);
 if(seed.accountKey!==input.accountKey||seed.selectedId!==input.assignmentId||seed.revision!==input.expectedRevision+1||seed.choices.find(choice=>choice.id===input.assignmentId)?.version!==input.choiceVersion)throw Error('Platform role confirmation did not match the captured selection. Retry the same selection.');
 return seed;
}
