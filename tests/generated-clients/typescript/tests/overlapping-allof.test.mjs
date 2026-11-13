import assert from 'node:assert/strict';
import test from 'node:test';
import * as client from '../generated/overlapping-allof/dist/index.mjs';

const json = (value) => JSON.parse(JSON.stringify(value));
const schema = { properties: { keep_key: { type: 'string' } }, additionalProperties: false };
const child = { firstName: 'Grace', lastName: 'Hopper', jsonSchema: schema, nextNode: null };
const childWire = { first_name: 'Grace', last_name: 'Hopper', json_schema: schema, next_node: null };

for (const [name, value, wire] of [
  ['recursive optional objects', { config: { ...child, nextNode: child } }, { config: { ...childWire, next_node: childWire } }],
  ['arrays and dictionaries', { config: { ...child, children: [child], byKey: { keep_key: child } } }, { config: { ...childWire, children: [childWire], by_key: { keep_key: childWire } } }],
  ['absent optional fields', {}, {}],
]) {
  test(`overlapping allOf ${name}: serialize without stale wire keys`, () => {
    assert.deepEqual(json(client.overlapToJSON(value)), wire);
  });
  test(`overlapping allOf ${name}: deserialize without stale public keys`, () => {
    assert.deepEqual(json(client.overlapFromJSON(wire)), value);
  });
}

test('an authored model whose name collides with a view stays independent', () => {
  assert.deepEqual(client.overlapConfigToJSON({ authoredField: false }), { authored_field: false });
});

test('aliased arrays retain every nested field in both directions', () => {
  assert.deepEqual(json(client.arrayOverlapToJSON({ config: [child] })), { config: [childWire] });
  assert.deepEqual(json(client.arrayOverlapFromJSON({ config: [childWire] })), { config: [child] });
});

test('projected objects keep integer constraints in strict union selection', () => {
  const value = { config: child, count: 1 };
  const wire = { config: childWire, count: 1 };
  assert.deepEqual(json(client.numericChoiceToJSON(value)), wire);
  assert.deepEqual(json(client.numericChoiceFromJSON(wire)), value);
  assert.throws(() => client.numericChoiceToJSON({ ...value, count: 1.5 }));
  assert.throws(() => client.numericChoiceFromJSON({ ...wire, count: 1.5 }));
});
