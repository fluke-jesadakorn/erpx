import ts from 'typescript';
import { posix } from 'node:path';

export type WorkspaceSource = { modules: { specifier: string; runtime: boolean }[]; strings: string[]; paths: string[]; urls: string[] };
/** Parse syntax once; resolve every target against the current graph afterwards. */
export function parseWorkspaceSource(file: string, source: string): WorkspaceSource {
  const resources=/^(?:tests|fixtures|scripts)\//.test(file);
  const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
  const modules = new Map<string, boolean>(), strings = new Set<string>(), paths = new Set<string>(), urls = new Set<string>();
  const mark = (specifier: string, runtime: boolean) => modules.set(specifier, modules.get(specifier) === true || runtime);
  function visit(node: ts.Node) {
    if (resources && ts.isStringLiteralLike(node)) strings.add(node.text);
    if ((ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) && node.moduleSpecifier && ts.isStringLiteral(node.moduleSpecifier)) {
      const typeOnly = ts.isImportDeclaration(node)
        ? node.importClause?.isTypeOnly || node.importClause?.namedBindings && ts.isNamedImports(node.importClause.namedBindings) && !node.importClause.name && node.importClause.namedBindings.elements.length > 0 && node.importClause.namedBindings.elements.every(item => item.isTypeOnly)
        : node.isTypeOnly || node.exportClause && ts.isNamedExports(node.exportClause) && node.exportClause.elements.length > 0 && node.exportClause.elements.every(item => item.isTypeOnly);
      mark(node.moduleSpecifier.text, !typeOnly);
    }
    if (ts.isImportTypeNode(node) && ts.isLiteralTypeNode(node.argument) && ts.isStringLiteral(node.argument.literal)) mark(node.argument.literal.text, false);
    if (ts.isImportEqualsDeclaration(node) && ts.isExternalModuleReference(node.moduleReference) && node.moduleReference.expression && ts.isStringLiteral(node.moduleReference.expression)) mark(node.moduleReference.expression.text, !node.isTypeOnly);
    if (ts.isCallExpression(node)) {
      if ((node.expression.kind === ts.SyntaxKind.ImportKeyword || ts.isIdentifier(node.expression) && node.expression.text === 'require') && node.arguments.length && ts.isStringLiteralLike(node.arguments[0])) mark(node.arguments[0].text, true);
      if (resources && ts.isIdentifier(node.expression) && ['join', 'resolve', 'readdir', 'readdirSync', 'readFile', 'readFileSync'].includes(node.expression.text)) {
        const fragments: string[] = [];
        for (const argument of [...node.arguments].reverse()) { if (!ts.isStringLiteralLike(argument)) break; fragments.unshift(argument.text); }
        if (fragments.length) paths.add(posix.normalize(fragments.join('/')));
      }
    }
    if (resources && ts.isNewExpression(node) && ts.isIdentifier(node.expression) && node.expression.text === 'URL' && node.arguments?.length && ts.isStringLiteralLike(node.arguments[0])) urls.add(node.arguments[0].text);
    ts.forEachChild(node, visit);
  }
  visit(tree);
  return { modules: [...modules].map(([specifier, runtime]) => ({ specifier, runtime })), strings: [...strings], paths: [...paths], urls: [...urls] };
}
