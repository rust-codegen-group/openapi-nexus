import assert from 'node:assert/strict';
import test from 'node:test';
import * as client from '../generated/optional-discriminator-union/dist/index.mjs';

const json = (value) => JSON.parse(JSON.stringify(value));
const payloadSchema = { properties: { nested_key: { type: 'string' } } };

for (const [name, value, wire] of [
  ['omitted variant_a tag', { payloadA: 'alpha', payloadASchema: payloadSchema }, { payload_a: 'alpha', payload_a_schema: payloadSchema }],
  ['explicit variant_a tag', { kind: 'variant_a', payloadA: 'alpha', payloadASchema: payloadSchema }, { kind: 'variant_a', payload_a: 'alpha', payload_a_schema: payloadSchema }],
  ...['variant_b_1', 'variant_b_2'].map((kind) => [
    kind,
    { kind, payloadB: 0 },
    { kind, payload_b: 0 },
  ]),
  ['singleton referenced tag', { kind: 'variant_c', payloadC: 'gamma' }, { kind: 'variant_c', payload_c: 'gamma' }],
  ['omitted variant_d tag', { payloadD: 'delta' }, { payload_d: 'delta' }],
  ['explicit variant_d tag', { kind: 'variant_d', payloadD: 'delta' }, { kind: 'variant_d', payload_d: 'delta' }],
]) {
  test(`optional/multi-valued discriminator ${name}: to wire`, () => {
    assert.deepEqual(json(client.unionToJSON(value)), wire);
  });
  test(`optional/multi-valued discriminator ${name}: from wire`, () => {
    assert.deepEqual(json(client.unionFromJSON(wire)), value);
  });
}

test('guards distinguish the legal variants', () => {
  const omitted = { payloadA: 'alpha', payloadASchema: payloadSchema };
  const explicit = { kind: 'variant_a', payloadA: 'alpha', payloadASchema: payloadSchema };
  const early = { kind: 'variant_b_1', payloadB: 0 };
  const late = { kind: 'variant_b_2', payloadB: 0 };
  assert.equal(client.isVariantA(omitted), true);
  assert.equal(client.isVariantA(explicit), true);
  assert.equal(client.isVariantA(early), false);
  assert.equal(client.isVariantB1(early), true);
  assert.equal(client.isVariantB1(late), true);
  assert.equal(client.isVariantA(late), false);
  assert.equal(client.isVariantC({ kind: 'variant_c', payloadC: 'gamma' }), true);
  assert.equal(client.isVariantC(explicit), false);
  assert.equal(client.isVariantD({ payloadD: 'delta' }), true);
  assert.equal(client.isVariantA({ payloadD: 'delta' }), false);
});

test('rejects invalid discriminator values at runtime', () => {
  assert.throws(() => client.unionFromJSON({ kind: null, payload_a: 'alpha' }));
  assert.throws(() => client.unionFromJSON({ kind: 'variant_unknown', payload_b: 0 }));
  assert.throws(() => client.unionToJSON({ kind: null, payloadA: 'alpha', payloadASchema: {} }));
  assert.throws(() => client.unionToJSON({ kind: 'variant_unknown', payloadB: 0 }));
});

for (const [name, value, wire] of [
  ['omitted variant_a tag through container', { payloadA: 'alpha', payloadASchema: payloadSchema }, { payload_a: 'alpha', payload_a_schema: payloadSchema }],
  ['early variant_b tag through container', { kind: 'variant_b_1', payloadB: 0 }, { kind: 'variant_b_1', payload_b: 0 }],
]) {
  test(`${name}: nested union to wire`, () => {
    assert.deepEqual(json(client.unionContainerToJSON(value)), wire);
  });
  test(`${name}: nested union from wire`, () => {
    assert.deepEqual(json(client.unionContainerFromJSON(wire)), value);
  });
}
