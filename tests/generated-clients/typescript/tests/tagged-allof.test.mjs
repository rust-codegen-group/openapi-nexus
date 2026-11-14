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
const options = { allowedSources: ['direct'], deferred: false, outputShape: { keep_key: true } };
const optionsWire = { allowed_sources: ['direct'], deferred: false, output_shape: { keep_key: true } };
const first = { type: 'variant_a', label: 'item', details: '', arguments: parameters, strictMode: false, ...options };
const firstWire = { type: 'variant_a', label: 'item', details: '', arguments: parameters, strict_mode: false, ...optionsWire };
const third = {
  type: 'variant_c',
  configuration: { type: 'nested', location: '/root', entries: [{ entryPath: '/entry' }] },
};
const thirdWire = {
  type: 'variant_c',
  configuration: { type: 'nested', location: '/root', entries: [{ entry_path: '/entry' }] },
};
const second = { type: 'variant_b', label: 'group', details: '', members: [first, third] };
const secondWire = { type: 'variant_b', label: 'group', details: '', members: [firstWire, thirdWire] };

for (const [name, value, wire] of [
  ['variant A', first, firstWire],
  ['recursive variant B', second, secondWire],
  ['variant C with nested config', third, thirdWire],
  ['tag and payload in variant D', { type: 'variant_d', inputShape: parameters, ...options }, { type: 'variant_d', input_shape: parameters, ...optionsWire }],
  ['inline-only payload', { type: 'variant_e', inlineLabel: 'kept' }, { type: 'variant_e', inline_label: 'kept' }],
  ['multiple references', { type: 'variant_f', displayName: 'kept', ...options }, { type: 'variant_f', display_name: 'kept', ...optionsWire }],
  ['outer properties', { type: 'variant_g', outerLabel: 'kept', outerFlag: false, outerCount: 0, ...options }, { type: 'variant_g', outer_label: 'kept', outer_flag: false, outer_count: 0, ...optionsWire }],
  ['nested sibling properties', { type: 'variant_k', innerLabel: 'in', outerLabel: 'out' }, { type: 'variant_k', inner_label: 'in', outer_label: 'out' }],
  ['overlapping nested objects', { type: 'variant_j', data: { firstValue: 'one', secondValue: 'two' } }, { type: 'variant_j', data: { first_value: 'one', second_value: 'two' } }],
  ['outer requiredness', { type: 'variant_i', label: 'required' }, { type: 'variant_i', label: 'required' }],
  ['reference-plus-tag control', { type: 'variant_h', ...options }, { type: 'variant_h', ...optionsWire }],
  ['unit variant control', { type: 'variant_l' }, { type: 'variant_l' }],
]) {
  test(`tagged allOf ${name}: serialization retains payload and renames fields`, () => {
    assert.deepEqual(json(client.variantToJSON(value)), wire);
  });
  test(`tagged allOf ${name}: deserialization retains payload and renames fields`, () => {
    assert.deepEqual(json(client.variantFromJSON(wire)), value);
  });
}

test('result wrappers retain nested variant declarations', () => {
  const value = [{ type: 'result', requestId: 'request-1', members: [first, second] }];
  const wire = [{ type: 'result', request_id: 'request-1', members: [firstWire, secondWire] }];
  assert.deepEqual(json(client.containerInputToJSON(value)), wire);
  assert.deepEqual(json(client.containerInputFromJSON(wire)), value);
});

test('a containing object retains declarations and nested result members', () => {
  const value = { members: [second], input: [{ type: 'result', requestId: 'request-1', members: [first] }] };
  const wire = { members: [secondWire], input: [{ type: 'result', request_id: 'request-1', members: [firstWire] }] };
  assert.deepEqual(json(client.containerToJSON(value)), wire);
  assert.deepEqual(json(client.containerFromJSON(wire)), value);
});

test('nested variants recurse without rewriting arbitrary JSON schema keys', () => {
  const value = { type: 'variant_b', label: 'outer', members: [second] };
  const wire = { type: 'variant_b', label: 'outer', members: [secondWire] };
  assert.deepEqual(json(client.variantToJSON(value)), wire);
  assert.deepEqual(json(client.variantFromJSON(wire)), value);
  assert.equal(client.isVariantA(first), true);
  assert.equal(client.isVariantB(first), false);
});
