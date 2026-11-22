import assert from 'node:assert/strict';
import test from 'node:test';
import * as client from '../generated/inferred-union-selection/dist/index.mjs';
import * as strict from '../generated/inferred-union-strict/dist/index.mjs';
import * as preserve from '../generated/inferred-union-preserve/dist/index.mjs';
import * as raw from '../generated/inferred-union-raw/dist/index.mjs';

const through = (from, to, wire) => assert.deepEqual(to(from(wire)), wire);

const leaf = { kind: 'leaf', label: 'end', count: 2 };
const tree = { kind: 'node', children: { first_child: { children: { next_child: leaf } } } };

test('a declared discriminator is not an extra dictionary value, even in recursive unions', () => {
  through(client.treeFromJSON, client.treeToJSON, leaf);
  through(client.treeFromJSON, client.treeToJSON, tree);
  through(client.envelopeFromJSON, client.envelopeToJSON, { kind: 'tree', tree });
  assert.throws(() => client.treeFromJSON({ ...leaf, count: 'wrong' }));
});

test('an allOf conjunct still applies its own extra-property constraint', () => {
  const wire = { label: 'a', count_extra: 3 };
  through(client.composedFromJSON, client.composedToJSON, wire);
  assert.throws(() => client.composedFromJSON({ ...wire, kind: 'a' }));
});

test('tag null is present and finite labels guide tagless selection', () => {
  for (const wire of [
    { kind: null, label: 'gamma' },
    { label: 'alpha' },
    { label: 'beta' },
    { label: 'shared' },
    { kind: 'b_alt', label: 'shared' },
  ]) {
    through(client.choiceFromJSON, client.choiceToJSON, wire);
  }
  assert.equal(client.isA({ label: 'beta' }), false);
  assert.equal(client.isB({ label: 'beta' }), true);
  assert.throws(() => client.choiceFromJSON({ label: 'gamma' }));
});

test('strict rejects ambiguous authored branches but permits either explicit value', () => {
  assert.throws(() => strict.choiceFromJSON({ label: 'shared' }), /Ambiguous Choice tagged union/);
  assert.throws(() => strict.choiceToJSON({ label: 'shared' }), /Ambiguous Choice tagged union/);
  for (const kind of ['b', 'b_alt']) {
    through(strict.choiceFromJSON, strict.choiceToJSON, { kind, label: 'shared' });
  }
});

test('preserve guards inspect authored wire fields without exporting converters', () => {
  assert.equal(preserve.isA({ label: 'beta' }), false);
  assert.equal(preserve.isB({ label: 'beta' }), true);
  assert.equal(preserve.isC({ kind: null, label: 'gamma' }), true);
  assert.equal(preserve.isC({ label: 'gamma' }), false);
  assert.equal('choiceFromJSON' in preserve, false);
});

test('common constraints do not promote an opaque fallback to concrete', () => {
  const wire = { scope: 'root', kind: 'typed', payload_value: 'a' };
  const selected = { scope: 'root', kind: 'typed', payloadValue: 'a' };
  assert.deepEqual(client.gatedChoiceFromJSON(wire), selected);
  assert.deepEqual(strict.gatedChoiceFromJSON(wire), selected);
  assert.deepEqual(strict.gatedChoiceToJSON(selected), wire);
  assert.throws(() => client.gatedChoiceFromJSON({ kind: 'typed', payload_value: 'a' }));

  const nested = { scope: 'root', other_value: 'a' };
  assert.deepEqual(client.gatedContainerFromJSON(nested), { scope: 'root', otherValue: 'a' });
});

test('raw accepts unchanged wire values instead of converting to camelCase', () => {
  const wire = { kind: 'variant_b_2', payload_b: 4 };
  const unmodelled = { kind: 'future', payload_b: 7 };
  for (const value of [wire, unmodelled]) {
    assert.equal(raw.unionFromJSON(value), value);
    assert.equal(raw.unionToJSON(value), value);
    assert.equal(raw.unionContainerFromJSON(value), value);
    assert.equal(raw.unionContainerToJSON(value), value);
  }
});
