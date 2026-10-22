/** Source edges retain the complete closure without repeating it for every app. */
const cached = new WeakMap();
function state(inventory){let value=cached.get(inventory);if(!value){value={closures:new WeakMap(),dependencies:new WeakMap(),sets:new WeakMap()};cached.set(inventory,value);}return value;}
const validId=(inventory,id)=>Number.isSafeInteger(id)&&id>=0&&id<inventory.closurePaths.length;
export function routeClosureIds(inventory,route){
 if(Array.isArray(route.closureIds))return route.closureIds;
 const cache=state(inventory);let found=cache.closures.get(route);if(found)return found;
 if(!Array.isArray(inventory.sourceEdges)||inventory.sourceEdges.length!==inventory.closurePaths.length||!Array.isArray(route.entryIds)||route.entryIds.some(id=>!validId(inventory,id)))throw new Error('UI_ROUTE_CLOSURE_REQUIRED');
 const seen=new Set(),pending=[...route.entryIds];for(const id of pending){if(seen.has(id))continue;seen.add(id);const edges=inventory.sourceEdges[id];if(!Array.isArray(edges)||edges.some(edge=>!validId(inventory,edge)))throw new Error('UI_ROUTE_CLOSURE_REQUIRED');for(const edge of edges)if(!seen.has(edge))pending.push(edge);}
 found=[...seen].sort((a,b)=>a-b);cache.closures.set(route,found);return found;
}
export function routeDependencies(inventory, route) {
 if(Array.isArray(route.dependencies))return route.dependencies;
 const cache=state(inventory);let result=cache.dependencies.get(route);if(result)return result;
 const ids=Array.isArray(route.dependencyIds)?route.dependencyIds:routeClosureIds(inventory,route);
 if(ids.some(id=>!validId(inventory,id)))throw new Error('UI_ROUTE_DEPENDENCY_INVENTORY_REQUIRED');
 const sources=Array.isArray(route.dependencyIds)?null:new Set(inventory.sources.map(source=>source.source));
 result=ids.map(index=>inventory.closurePaths[index]).filter(path=>!sources||sources.has(path));cache.dependencies.set(route,result);return result;
}
export function routeDependsOn(inventory,route,source){const cache=state(inventory);let included=cache.sets.get(route);if(!included){included=new Set(routeDependencies(inventory,route));cache.sets.set(route,included);}return included.has(source);}
