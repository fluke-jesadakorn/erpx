/** An absent fixture row or projection field must fail the test at its use site. */
export function requiredFact<Value>(value:Value|null|undefined):Value {
  if(value===null||value===undefined)throw new Error('Required test fact is missing');
  return value;
}
export function recordField(value:object,key:string):unknown {
  if(!(key in value))throw new Error(`Required projection field is missing: ${key}`);
  return Reflect.get(value,key);
}
export function stringField(value:object,key:string):string {
  const field=recordField(value,key);
  if(typeof field!=='string')throw new Error(`Expected a string projection field: ${key}`);
  return field;
}
export function numberField(value:object,key:string):number {
  const field=recordField(value,key);
  if(typeof field!=='number'||!Number.isFinite(field))throw new Error(`Expected a finite numeric projection field: ${key}`);
  return field;
}
