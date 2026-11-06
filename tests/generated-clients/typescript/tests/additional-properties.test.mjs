import assert from 'node:assert/strict';
import test from 'node:test';
import * as client from '../generated/camel-case/dist/index.mjs';

const cases = [
  ['plainStringMap', { known_key: 'fixed', extra_dynamic: 'opaque' }],
  ['plainJsonMap', { known_key: 'fixed', extra_dynamic: { nested_key: ['opaque', null, 7] } }],
  ['nestedStringMap', { known: 'fixed', extra: { extra_dynamic: 'opaque' } }],
  ['nestedJsonMap', { known: 'fixed', extra: { extra_dynamic: { nested_key: ['opaque', null, 7] } } }],
  ['mixedStringMap', { known: 'fixed', extra_dynamic: 'opaque' }],
  ['mixedJsonMap', { known: 'fixed', extra_dynamic: { nested_key: ['opaque', null, 7] } }],
  ['mixedInlineAnyMap', { known: 'fixed', extra_dynamic: { nested_key: ['opaque', null, 7] } }],
  ['mixedBooleanMap', { known: 'fixed', extra_dynamic: { nested_key: ['opaque', null, 7] } }],
  ['mixedNumberMap', { known: 1, extra_dynamic: 2 }],
  ['allOfMixedStringMap', { known: 'fixed', extra_dynamic: 'opaque' }],
  ['searchItem', { type: 'web_search_call', id: 'ws-1', status: 'completed', opaque_engine_payload: { encrypted: 'opaque', results: [{ title: 'Python' }] } }],
  ['searchStreamEvent', { type: 'response.web_search_call.searching', sequence_number: 7, item_id: 'ws-1', opaque_engine_payload: { encrypted: 'opaque' } }],
];

for (const [name, input] of cases) {
  test(`${name} retains the complete wire payload`, () => {
    assert.deepEqual(client[`${name}ToJSON`](client[`${name}FromJSON`](input)), input);
  });
}

test('both conversion directions independently retain extensions', () => {
  const input = { known: 'fixed', extra_dynamic: 'opaque' };
  assert.deepEqual(client.mixedStringMapFromJSON(input), input);
  assert.deepEqual(client.mixedStringMapToJSON(input), input);
});

test('known fields rename without renaming opaque extension keys or values', () => {
  const input = { known_field: 'fixed', optional_field: null, extra_dynamic: { nested_key: 'opaque' } };
  const value = client.renamedMapFromJSON(input);
  assert.equal(value.knownField, 'fixed');
  assert.equal(value.optionalField, null);
  assert.deepEqual(value.extra_dynamic, { nested_key: 'opaque' });
  assert.equal(Object.hasOwn(value, 'known_field'), false);
  assert.equal(Object.hasOwn(value, 'extraDynamic'), false);
  assert.deepEqual(client.renamedMapToJSON(value), input);
});

test('omitted optional known properties remain absent', () => {
  const input = { known_field: 'fixed', extra_dynamic: 42 };
  const value = client.renamedMapFromJSON(input);
  assert.equal(Object.hasOwn(value, 'optionalField'), false);
  assert.deepEqual(client.renamedMapToJSON(value), input);
  assert.deepEqual(client.mixedStringMapToJSON(client.mixedStringMapFromJSON({ extra_dynamic: 'opaque' })), { extra_dynamic: 'opaque' });
});

test('camelCase name collisions preserve named-field access and the original extension', () => {
  const input = { known_field: 'fixed', knownField: { opaque_key: 'extension' }, optional_field: null, optionalField: 'extension' };
  const value = client.renamedMapFromJSON(input);
  assert.equal(value.knownField, 'fixed');
  assert.equal(value.optionalField, null);
  assert.deepEqual(client.renamedMapToJSON(value), input);
  assert.deepEqual(client.renamedMapToJSON({ ...value }), input);
});

test('an extension colliding with an absent optional field does not create that field', () => {
  const input = { known_field: 'fixed', optionalField: { opaque_key: 'extension' } };
  const value = client.renamedMapFromJSON(input);
  assert.equal(Object.hasOwn(value, 'optionalField'), false);
  assert.deepEqual(client.renamedMapToJSON(value), input);
  assert.deepEqual(client.renamedMapToJSON({ ...value }), input);
});

test('allOf dictionaries preserve renamed known fields and colliding extensions', () => {
  const input = { known_field: 'fixed', knownField: 'extension', optionalField: { opaque_key: 7 } };
  const value = client.allOfRenamedMapFromJSON(input);
  assert.equal(value.knownField, 'fixed');
  assert.equal(Object.hasOwn(value, 'known_field'), false);
  assert.equal(Object.hasOwn(value, 'optionalField'), false);
  assert.deepEqual(client.allOfRenamedMapToJSON({ ...value }), input);
});

test('compatible allOf dictionary contracts preserve payloads and named-field access', () => {
  const input = { known_field: 'fixed', other_field: 'other', extra_dynamic: 'opaque' };
  const value = client.heterogeneousAllOfFromJSON(input);
  assert.equal(value.knownField, 'fixed');
  assert.equal(value.otherField, 'other');
  assert.equal(Object.hasOwn(value, 'known_field'), false);
  assert.equal(Object.hasOwn(value, 'other_field'), false);
  assert.deepEqual(client.heterogeneousAllOfToJSON(value), input);
  assert.deepEqual(client.heterogeneousAllOfToJSON({ knownField: 'fixed', otherField: 'other', extra_dynamic: 'opaque' }), input);
});

test('compatible allOf dictionaries retain both colliding extensions', () => {
  const input = { known_field: 'fixed', other_field: 'other', knownField: 'opaque-A', otherField: 'opaque-B' };
  const value = client.heterogeneousAllOfFromJSON(input);
  assert.equal(value.knownField, 'fixed');
  assert.equal(value.otherField, 'other');
  assert.deepEqual(client.heterogeneousAllOfToJSON({ ...value }), input);
});

for (const key of ['started', 'finished']) {
  test(`open external ${key} wrappers convert nested models and preserve opaque extensions`, () => {
    const input = { [key]: { display_name: 'Ada' }, extra_dynamic: { opaque_key: 'payload' } };
    const value = client.openExternalEventFromJSON(input);
    assert.equal(value[key].displayName, 'Ada');
    assert.equal(Object.hasOwn(value[key], 'display_name'), false);
    assert.deepEqual(value.extra_dynamic, { opaque_key: 'payload' });
    assert.deepEqual(client.openExternalEventToJSON(value), input);
    assert.deepEqual(client.openExternalEventToJSON({ [key]: { displayName: 'Ada' }, extra_dynamic: { opaque_key: 'payload' } }), input);
  });

  test(`typed external ${key} extensions follow alias conversions without renaming keys`, () => {
    const input = { [key]: { display_name: 'Ada' }, extra_dynamic: { display_name: 'Grace' } };
    const value = client.typedExternalEventFromJSON(input);
    assert.deepEqual(value, { [key]: { displayName: 'Ada' }, extra_dynamic: { displayName: 'Grace' } });
    assert.deepEqual(client.typedExternalEventToJSON(value), input);
    assert.deepEqual(client.typedExternalEventToJSON({ [key]: { displayName: 'Ada' }, extra_dynamic: { displayName: 'Grace' } }), input);
  });
}

test('closed external wrappers keep their existing extension policy', () => {
  const value = client.closedExternalEventFromJSON({ started: { display_name: 'Ada' }, extra_dynamic: 'not declared' });
  assert.deepEqual(value, { started: { displayName: 'Ada' } });
  assert.deepEqual(client.closedExternalEventToJSON(value), { started: { display_name: 'Ada' } });
});

test('a closed alternative stays closed when another external alternative is open', () => {
  const value = client.mixedExternalEventFromJSON({ finished: { display_name: 'Ada' }, extra_dynamic: 'not declared' });
  assert.deepEqual(value, { finished: { displayName: 'Ada' } });
  assert.deepEqual(client.mixedExternalEventToJSON(value), { finished: { display_name: 'Ada' } });
});

test('colliding typed extensions are converted once in each direction', () => {
  const input = { known_count: 1, knownCount: { display_name: 'Ada' } };
  const value = client.mixedModelMapFromJSON(input);
  assert.equal(value.knownCount, 1);
  assert.deepEqual(value[Symbol.for('openapi-nexus.additionalProperties')], { knownCount: { displayName: 'Ada' } });
  assert.deepEqual(client.mixedModelMapToJSON({ ...value }), input);
});

test('prototype-sensitive extension keys stay own data properties', () => {
  const input = JSON.parse('{"known":"fixed","__proto__":{"polluted":true},"constructor":"opaque","prototype":[null,7]}');
  const value = client.mixedJsonMapFromJSON(input);
  assert.equal(Object.getPrototypeOf(value), Object.prototype);
  assert.equal(Object.hasOwn(value, '__proto__'), true);
  assert.equal(Object.prototype.polluted, undefined);
  assert.deepEqual(client.mixedJsonMapToJSON(value), input);
});

test('quoted extension keys and prototype-sensitive declared properties are safe', () => {
  const input = JSON.parse('{"kebab-field":"fixed","__proto__":"declared","quoted\'extension":"opaque"}');
  const value = client.quotedMapFromJSON(input);
  assert.equal(Object.getPrototypeOf(value), Object.prototype);
  assert.deepEqual(client.quotedMapToJSON(value), input);
});

for (const [name, input, expected] of [
  ['aliasMap', { known_field: 'fixed', dynamic_key: { display_name: 'Ada' } }, { knownField: 'fixed', dynamic_key: { displayName: 'Ada' } }],
  ['aliasArrayMap', { known_count: 1, dynamic_key: [{ display_name: 'Ada' }] }, { knownCount: 1, dynamic_key: [{ displayName: 'Ada' }] }],
  ['aliasNullableMap', { known_count: 1, dynamic_key: { display_name: 'Ada' }, null_key: null }, { knownCount: 1, dynamic_key: { displayName: 'Ada' }, null_key: null }],
  ['plainModelMap', { dynamic_key: { display_name: 'Ada' } }, { dynamic_key: { displayName: 'Ada' } }],
  ['mixedModelMap', { known_count: 1, dynamic_key: { display_name: 'Ada' } }, { knownCount: 1, dynamic_key: { displayName: 'Ada' } }],
  ['arrayModelMap', { known_count: 1, dynamic_key: [{ display_name: 'Ada' }] }, { knownCount: 1, dynamic_key: [{ displayName: 'Ada' }] }],
  ['nestedModelMap', { known_count: 1, dynamic_key: { nested_key: { display_name: 'Ada' } } }, { knownCount: 1, dynamic_key: { nested_key: { displayName: 'Ada' } } }],
  ['nullableModelMap', { known_count: 1, dynamic_key: { display_name: 'Ada' }, null_key: null }, { knownCount: 1, dynamic_key: { displayName: 'Ada' }, null_key: null }],
  ['recursiveMap', { node_label: 'root', child_key: { node_label: 'leaf' } }, { nodeLabel: 'root', child_key: { nodeLabel: 'leaf' } }],
]) {
  test(`${name} converts typed additional values without renaming dictionary keys`, () => {
    const value = client[`${name}FromJSON`](input);
    assert.deepEqual(value, expected);
    assert.deepEqual(client[`${name}ToJSON`](value), input);
    assert.deepEqual(client[`${name}ToJSON`](expected), input);
  });
}

test('additionalProperties false keeps the existing closed-object behavior', () => {
  const value = client.closedObjectFromJSON({ known_field: 'fixed', extra_dynamic: 'not declared' });
  assert.deepEqual(value, { knownField: 'fixed' });
  assert.deepEqual(client.closedObjectToJSON(value), { known_field: 'fixed' });
});
