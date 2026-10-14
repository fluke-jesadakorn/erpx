import {routeContextHref,type RouteSearch} from "@/lib/shared/route-context";
export type WorkspaceTopic={id:string;title:string;description?:string;path:string;query:Record<string,string>};
export const sectionWorkspaceTopics={
 administration:[
  ...[['templates','Routes'],['issues','Routing issues'],['history','Routing history']].map(([tab,title])=>({id:`approvals:${tab}`,title,path:'settings',query:{section:'approvals',tab}})),
  ...[['identity','Brand identity'],['media','Brand media'],['history','Brand version history']].map(([tab,title])=>({id:`brand:${tab}`,title,path:'settings',query:{section:'brand',tab}})),
  ...[['ai','AI providers'],['usage','Usage & billing'],['feedback','Help & feedback']].map(([section,title])=>({id:section,title,path:'settings',query:{section}})),
 ],
 assets:[{id:'register',title:'Asset register',path:'assets',query:{view:'register'}},{id:'maintenance',title:'Maintenance work',path:'assets',query:{view:'maintenance'}},{id:'plans',title:'Preventive plans',path:'assets',query:{view:'plans'}}],
 knowledge:[{id:'handbook:published',title:'Published handbook',path:'knowledge',query:{view:'handbook',section:'published'}},{id:'handbook:drafts',title:'Article drafts',path:'knowledge',query:{view:'handbook',section:'drafts'}},{id:'handbook:editors',title:'Editorial access',path:'knowledge',query:{view:'handbook',section:'editors'}},{id:'learning:courses',title:'Courses',path:'knowledge',query:{view:'learning',section:'courses'}},{id:'learning:assignments',title:'Learning assignments',path:'knowledge',query:{view:'learning',section:'assignments'}},{id:'learning:mine',title:'My learning',path:'knowledge',query:{view:'learning',section:'mine'}}],
 benefits:[{id:'plans',title:'Benefit plans',path:'benefits',query:{view:'plans'}},{id:'enrollments',title:'Enrollments',path:'benefits',query:{view:'enrollments'}},{id:'mine',title:'My benefits',path:'benefits',query:{view:'mine'}}],
 catalog:[{id:'products',title:'Products',path:'commerce',query:{view:'products'}},{id:'warehouses',title:'Warehouses',path:'commerce',query:{view:'warehouses'}},{id:'pricing',title:'Price lists',path:'commerce',query:{view:'pricing'}}],
 finance:[...['checklist','chart','profile','opening','books','tools','policy','ai'].map((id,index)=>({id,title:['Readiness','Charts & accounts','Company accounting policy','Opening balances','Books & dimensions','Company tools','Go-live review','AI preparation'][index],path:'finance/setup',query:{step:id}}))],
 projects:[...['overview','tasks','structure','actuals','handoff','documents'].map((id,index)=>({id,title:['Overview','Tasks','Work structure','Actuals','Handoff','Documents'][index],path:'projects',query:{view:id}}))],
} satisfies Record<string,WorkspaceTopic[]>;
export function workspaceTopicLinks(base:string,search:RouteSearch,topics:readonly WorkspaceTopic[]){return topics.map(topic=>({id:topic.id,title:topic.title,description:'description' in topic?topic.description:undefined,href:routeContextHref(`${base}/${topic.path}`,search,{detailKind:null,detailId:null,detailArgs:null,detailPage:null,detailMode:null,cursor:null,...(topic.path==='settings'?{tab:null}:{}),...topic.query})}));}
export function legacyWorkspaceDestination(base:string,path:string,search:RouteSearch){
 if(path==='maintenance')return routeContextHref(`${base}/assets`,search,{view:'maintenance'});
 if(path==='handbook')return routeContextHref(`${base}/knowledge`,search,{view:'handbook',section:'published'});
 if(path==='learning')return routeContextHref(`${base}/knowledge`,search,{view:'learning',section:search.view==='mine'?'mine':'courses'});
 if(path==='brand')return routeContextHref(`${base}/settings`,search,{section:'brand',tab:'identity'});
 return null;
}

export function workspaceSearchContext(search:RouteSearch,patch:Record<string,string>={}){return {...Object.fromEntries(Object.entries(search).filter((entry):entry is [string,string|string[]]=>entry[1]!==undefined&&!(["q","cursor","detailKind","detailId","detailArgs","detailPage","detailMode"].includes(entry[0])))),...patch};}

/** Presentation eligibility mirrors each owner reader; a combined workspace grants no authority. */
export function assetTopicsFor(modules:readonly string[],canRead:boolean){return sectionWorkspaceTopics.assets.filter(topic=>canRead&&modules.includes(topic.id==='register'?'assets':'maintenance'));}
export function knowledgeTopicsFor(access:{editor:boolean;owner:boolean},learning:boolean){return sectionWorkspaceTopics.knowledge.filter(topic=>topic.id.startsWith('learning:')?learning:topic.id==='handbook:drafts'?access.editor:topic.id==='handbook:editors'?access.owner:true);}
