import assert from 'node:assert/strict';
import test from 'node:test';
import * as client from '../generated/union-default/dist/index.mjs';
import * as strict from '../generated/union-strict/dist/index.mjs';
import * as raw from '../generated/union-raw/dist/index.mjs';

const clientValue = { name: 'weather', inputSchema: { type: 'object', opaque_key: { keep_name: true } } };
const clientWire = { name: 'weather', input_schema: { type: 'object', opaque_key: { keep_name: true } } };
const searchValue = { type: 'web_search_20250305', name: 'web_search', maxUses: 1 };
const searchWire = { type: 'web_search_20250305', name: 'web_search', max_uses: 1 };

for (const [name, value, wire] of [['client', clientValue, clientWire], ['search', searchValue, searchWire]]) {
  test(`${name} union conversion independently matches its member in both directions`, () => {
    assert.deepEqual(client.toolFromJSON(wire), value);
    assert.deepEqual(client.toolToJSON(value), wire);
    assert.deepEqual(strict.toolFromJSON(wire), value);
    assert.deepEqual(strict.toolToJSON(value), wire);
  });
}

test('a containing request converts its typed union elements', () => {
  const value = { tools: [clientValue, searchValue] };
  const wire = { tools: [clientWire, searchWire] };
  assert.deepEqual(client.toolRequestFromJSON(wire), value);
  assert.deepEqual(client.toolRequestToJSON(value), wire);
});

test('nullable and alias unions delegate to the same conversion', () => {
  assert.equal(client.nullableToolFromJSON(null), null);
  assert.equal(client.nullableToolToJSON(null), null);
  assert.deepEqual(client.nullableToolFromJSON(clientWire), clientValue);
  assert.deepEqual(client.toolAliasToJSON(searchValue), searchWire);
});

test('array and dictionary alternatives convert values, not dictionary keys', () => {
  assert.deepEqual(client.toolListFromJSON([clientWire]), [clientValue]);
  assert.deepEqual(client.toolListToJSON([searchValue]), [searchWire]);
  assert.deepEqual(client.toolMapFromJSON({ opaque_key: clientWire }), { opaque_key: clientValue });
  assert.deepEqual(client.toolMapToJSON({ opaque_key: searchValue }), { opaque_key: searchWire });
});

test('unconstrained JSON remains a fallback after a concrete convertible member', () => {
  assert.deepEqual(client.openToolFromJSON(clientWire), clientValue);
  assert.deepEqual(client.openToolToJSON(clientValue), clientWire);
  const unmatched = { arbitrary_key: [7, null, 'opaque'] };
  assert.deepEqual(client.openToolFromJSON(unmatched), unmatched);
  assert.deepEqual(client.openToolToJSON(unmatched), unmatched);
});

test('default ambiguous selection is deterministic and unmatched values stay intact', () => {
  assert.deepEqual(client.ambiguousToolFromJSON(clientWire), clientValue);
  assert.deepEqual(client.ambiguousToolToJSON(clientValue), clientWire);
  const unmatched = { arbitrary_key: true };
  assert.deepEqual(client.toolFromJSON(unmatched), unmatched);
  assert.deepEqual(client.toolToJSON(unmatched), unmatched);
});

test('strict policy rejects ambiguous and unmatched shapes without treating raw fallback as ambiguity', () => {
  assert.throws(() => strict.ambiguousToolFromJSON(clientWire), TypeError);
  assert.throws(() => strict.ambiguousToolToJSON(clientValue), TypeError);
  assert.throws(() => strict.toolFromJSON({ arbitrary_key: true }), TypeError);
  assert.throws(() => strict.toolToJSON({ arbitrary_key: true }), TypeError);
  assert.deepEqual(strict.openToolFromJSON(clientWire), clientValue);
});

test('required literals, primitive kinds, and own fields determine member matches', () => {
  assert.throws(() => strict.toolFromJSON({ type: 'wrong', name: 'web_search', max_uses: 1 }), TypeError);
  assert.throws(() => strict.toolFromJSON({ name: 7, input_schema: {} }), TypeError);
  assert.throws(() => strict.toolFromJSON(Object.create(clientWire)), TypeError);
});

test('raw policy explicitly preserves the original representation', () => {
  assert.equal(raw.toolFromJSON(clientWire), clientWire);
  assert.equal(raw.toolToJSON(searchWire), searchWire);
  assert.deepEqual(raw.toolRequestToJSON({ tools: [clientWire, searchWire] }), { tools: [clientWire, searchWire] });
});
