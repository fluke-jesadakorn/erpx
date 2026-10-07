import { learningCourseSchema, type LearningCourseInput } from "../domain/course";
import { type Clock } from "../../shared";
export interface LearningCourseInputPorts {
 clock:Clock;
 authorize():Promise<void>;
 replay(key:string,operation:string,id:string,input:LearningCourseInput):Promise<Record<string,unknown>|null>;
 persist(command:{key:string;operation:string;id:string;input:LearningCourseInput;now:number;result:{id:string;status:string;version:number}}):Promise<Record<string,unknown>>;
}
export function createLearningCourseApplication(ports:LearningCourseInputPorts){return async(raw:unknown,key:string)=>{
 await ports.authorize();
 const input=learningCourseSchema.parse(raw);
 
 const id="learning-course-"+key,operation="learning.course.create";
 const prior=await ports.replay(key,operation,id,input);if(prior)return prior;
 
 const result={id,status:"draft",version:1};
 return ports.persist({key,operation,id,input,now:ports.clock.now(),result});
};}
