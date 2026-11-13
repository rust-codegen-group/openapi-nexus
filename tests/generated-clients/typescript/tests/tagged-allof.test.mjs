import assert from 'node:assert/strict';
import test from 'node:test';
import * as client from '../generated/tagged-allof/dist/index.mjs';

// Assertions compare actual JSON payloads independently in each direction, not
// only a round trip that could conceal the same lossy behavior in both codecs.
const json = (value) => JSON.parse(JSON.stringify(value));
const parameters = {
  type: 'object',
  properties: { title: { type: 'string' }, keep_key: { enum: [0, false, null] } },
  required: ['title'],
};
const options = { allowedCallers: ['direct'], deferLoading: false, outputSchema: { keep_key: true } };
const optionsWire = { allowed_callers: ['direct'], defer_loading: false, output_schema: { keep_key: true } };
const fn = { type: 'function', name: 'create_event', description: '', parameters, strict: false, ...options };
const fnWire = { type: 'function', name: 'create_event', description: '', parameters, strict: false, ...optionsWire };
const shell = {
  type: 'shell',
  environment: { type: 'local', workingDirectory: '/workspace', skills: [{ skillPath: '/skills/calendar' }] },
};
const shellWire = {
  type: 'shell',
  environment: { type: 'local', working_directory: '/workspace', skills: [{ skill_path: '/skills/calendar' }] },
};
const namespace = { type: 'namespace', name: 'calendar', description: '', tools: [fn, shell] };
const namespaceWire = { type: 'namespace', name: 'calendar', description: '', tools: [fnWire, shellWire] };

for (const [name, value, wire] of [
  ['function', fn, fnWire],
  ['recursive namespace', namespace, namespaceWire],
  ['shell with nested environment', shell, shellWire],
  ['tag and payload in one member', { type: 'custom', inputSchema: parameters, ...options }, { type: 'custom', input_schema: parameters, ...optionsWire }],
  ['inline-only payload', { type: 'inline_only', inlineName: 'kept' }, { type: 'inline_only', inline_name: 'kept' }],
  ['multiple references', { type: 'multiple_refs', displayName: 'kept', ...options }, { type: 'multiple_refs', display_name: 'kept', ...optionsWire }],
  ['outer properties', { type: 'outer_properties', outerName: 'kept', outerFlag: false, outerCount: 0, ...options }, { type: 'outer_properties', outer_name: 'kept', outer_flag: false, outer_count: 0, ...optionsWire }],
  ['nested sibling properties', { type: 'nested_fields', innerName: 'in', outerName: 'out' }, { type: 'nested_fields', inner_name: 'in', outer_name: 'out' }],
  ['overlapping nested objects', { type: 'overlap', config: { firstName: 'Ada', lastName: 'Lovelace' } }, { type: 'overlap', config: { first_name: 'Ada', last_name: 'Lovelace' } }],
  ['outer requiredness', { type: 'outer_required', name: 'required' }, { type: 'outer_required', name: 'required' }],
  ['reference-plus-tag control', { type: 'simple', ...options }, { type: 'simple', ...optionsWire }],
  ['unit variant control', { type: 'other' }, { type: 'other' }],
]) {
  test(`tagged allOf ${name}: serialization retains payload and renames fields`, () => {
    assert.deepEqual(json(client.toolToJSON(value)), wire);
  });
  test(`tagged allOf ${name}: deserialization retains payload and renames fields`, () => {
    assert.deepEqual(json(client.toolFromJSON(wire)), value);
  });
}

test('search-output wrappers retain discovered function and namespace declarations', () => {
  const value = [{ type: 'tool_search_output', callId: 'search-1', tools: [fn, namespace] }];
  const wire = [{ type: 'tool_search_output', call_id: 'search-1', tools: [fnWire, namespaceWire] }];
  assert.deepEqual(json(client.inputToJSON(value)), wire);
  assert.deepEqual(json(client.inputFromJSON(wire)), value);
});

test('a containing request retains declarations and nested search-output tools', () => {
  const value = { tools: [namespace], input: [{ type: 'tool_search_output', callId: 'search-1', tools: [fn] }] };
  const wire = { tools: [namespaceWire], input: [{ type: 'tool_search_output', call_id: 'search-1', tools: [fnWire] }] };
  assert.deepEqual(json(client.envelopeToJSON(value)), wire);
  assert.deepEqual(json(client.envelopeFromJSON(wire)), value);
});

test('nested namespaces recurse without rewriting arbitrary JSON schema keys', () => {
  const value = { type: 'namespace', name: 'outer', tools: [namespace] };
  const wire = { type: 'namespace', name: 'outer', tools: [namespaceWire] };
  assert.deepEqual(json(client.toolToJSON(value)), wire);
  assert.deepEqual(json(client.toolFromJSON(wire)), value);
  assert.equal(client.isFunction(fn), true);
  assert.equal(client.isNamespace(fn), false);
});
