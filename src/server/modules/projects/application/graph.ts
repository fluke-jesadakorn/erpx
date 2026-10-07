import { buildProjectGraph, type ProjectGraphSource, type ProjectGraphViewer, type Project } from "../domain/graph";
export interface ProjectGraphPorts {
    read(projectId: string): Promise<ProjectGraphSource>;
    viewer(project: Project): ProjectGraphViewer;
}
export function createProjectGraphApplication(ports: ProjectGraphPorts) { return async (projectId: string) => { const source = await ports.read(projectId); return buildProjectGraph(source, ports.viewer(source.project)); }; }
