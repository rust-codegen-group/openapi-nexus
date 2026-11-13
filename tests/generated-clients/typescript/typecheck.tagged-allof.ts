import type { Tool, Tool$Wire } from './generated/tagged-allof/dist/index.mjs';
import type { Tool as PreservedTool } from './generated/tagged-allof-preserve/dist/index.mjs';
import { isFunction, isNamespace } from './generated/tagged-allof/dist/index.mjs';
import type { Overlap, Overlap$Wire, OverlapConfig } from './generated/overlapping-allof/dist/index.mjs';

const fn: Tool = { type: 'function', name: 'create_event', parameters: {}, strict: false, deferLoading: false };
const fnWire: Tool$Wire = { type: 'function', name: 'create_event', parameters: {}, strict: false, defer_loading: false };
const namespace: Tool = { type: 'namespace', name: 'calendar', tools: [fn] };
const namespaceWire: Tool$Wire = { type: 'namespace', name: 'calendar', tools: [fnWire] };
const shell: Tool = { type: 'shell', environment: { type: 'local', workingDirectory: '/workspace', skills: [{ skillPath: '/skills/calendar' }] } };
const shellWire: Tool$Wire = { type: 'shell', environment: { type: 'local', working_directory: '/workspace', skills: [{ skill_path: '/skills/calendar' }] } };
const preserved: PreservedTool = fnWire;
const preservedNamespace: PreservedTool = namespaceWire;
const preservedShell: PreservedTool = shellWire;
const inline: Tool = { type: 'inline_only', inlineName: 'kept' };
const multiple: Tool = { type: 'multiple_refs', displayName: 'kept', deferLoading: false };
const outer: Tool = { type: 'outer_properties', outerName: 'kept', outerCount: 0 };
const custom: Tool = { type: 'custom', inputSchema: { keep_key: true } };
const overlap: Tool = { type: 'overlap', config: { firstName: 'Ada', lastName: 'Lovelace' } };
const overlapWire: Tool$Wire = { type: 'overlap', config: { first_name: 'Ada', last_name: 'Lovelace' } };
const required: Tool = { type: 'outer_required', name: 'required' };
const nested: Tool = { type: 'nested_fields', innerName: 'in', outerName: 'out' };
const recursive: Overlap = { config: { firstName: 'Ada', lastName: 'Lovelace', nextNode: null } };
const recursiveWire: Overlap$Wire = { config: { first_name: 'Ada', last_name: 'Lovelace', next_node: null } };
const authored: OverlapConfig = { authoredField: true };

function expectType<T>(_value: T): void {}
if (isFunction(fn)) {
  expectType<string>(fn.name);
  expectType<boolean | undefined>(fn.strict);
  expectType<Readonly<Record<string, unknown>>>(fn.parameters);
}
if (isNamespace(namespace)) {
  expectType<readonly Tool[]>(namespace.tools);
}

// @ts-expect-error Function name remains required after allOf composition.
const missingName: Tool = { type: 'function', parameters: {} };
// @ts-expect-error Namespace declarations retain their required nested tools.
const missingTools: Tool = { type: 'namespace', name: 'calendar' };
// @ts-expect-error A shell declaration retains its required environment.
const missingEnvironment: Tool = { type: 'shell' };
// @ts-expect-error Common options must retain their ergonomic spelling.
const invalidErgonomic: Tool = { type: 'function', name: 'f', parameters: {}, defer_loading: false };
// @ts-expect-error Composed wire payloads keep the nested wire field spelling.
const invalidWire: Tool$Wire = { type: 'custom', inputSchema: {} };
// @ts-expect-error Preserve-mode payloads do not acquire ergonomic field names.
const invalidPreserved: PreservedTool = { type: 'inline_only', inlineName: 'wrong' };
// @ts-expect-error Tagged allOf nested arrays retain their readonly contract.
namespace.tools[0] = fn;
// @ts-expect-error Outer requiredness strengthens fields inherited from a reference.
const missingOuterRequired: Tool = { type: 'outer_required' };
// @ts-expect-error The wire declaration must retain the same requiredness.
const missingOuterRequiredWire: Tool$Wire = { type: 'outer_required' };
// @ts-expect-error Both nested allOf object constraints remain required.
const missingOverlapField: Tool = { type: 'overlap', config: { firstName: 'Ada' } };
// @ts-expect-error The merged recursive object retains both constraints.
const missingRecursiveField: Overlap = { config: { firstName: 'Ada' } };
// @ts-expect-error The recursive wire view does not accept public spellings.
const staleRecursiveKey: Overlap$Wire = { config: { first_name: 'Ada', lastName: 'Lovelace' } };

void [fn, fnWire, namespace, namespaceWire, shell, shellWire, preserved, preservedNamespace, preservedShell,
  inline, multiple, outer, custom, missingName, missingTools, missingEnvironment,
  invalidErgonomic, invalidWire, invalidPreserved, overlap, overlapWire, required,
  nested, missingOuterRequired, missingOuterRequiredWire, missingOverlapField,
  recursive, recursiveWire, authored, missingRecursiveField, staleRecursiveKey];
