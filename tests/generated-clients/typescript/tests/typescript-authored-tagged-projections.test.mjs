import assert from 'node:assert/strict';
import test from 'node:test';
import * as client from '../generated/authored-tagged/dist/index.mjs';
import * as strict from '../generated/authored-tagged-strict/dist/index.mjs';
import * as preserve from '../generated/authored-tagged-preserve/dist/index.mjs';
import * as raw from '../generated/authored-tagged-raw/dist/index.mjs';
import {
  composedTaggedFromJSON,
  composedTaggedToJSON,
} from '../generated/authored-tagged-composed/dist/index.mjs';

const wire = { tag_kind: 'scoped_a', scope_key: 'scope', root_key: 'root', a_value: 'a' };
const model = { tagKind: 'scoped_a', scopeKey: 'scope', rootKey: 'root', aValue: 'a' };

test('the shared required keys use each branch’s model names and root-only fields survive', () => {
  for (const sdk of [client, strict]) {
    assert.deepEqual(sdk.scopedChoiceFromJSON(wire), model);
    assert.deepEqual(sdk.scopedChoiceToJSON(model), wire);
    for (const [source, convert] of [
      [{ ...wire, root_key: undefined }, sdk.scopedChoiceFromJSON],
      [{ ...model, scopeKey: undefined }, sdk.scopedChoiceToJSON],
    ]) {
      assert.throws(() => convert(source), /Unknown variant/);
    }
  }
  assert.equal(client.isScopedA(model), true);
  assert.equal(preserve.isScopedA(wire), true);
  assert.equal(preserve.isScopedA(model), false);
  assert.equal('scopedChoiceFromJSON' in preserve, false);
  assert.equal(raw.scopedChoiceFromJSON(wire), wire);
  assert.equal(raw.scopedChoiceToJSON(wire), wire);
  assert.equal(raw.isScopedA(wire), true);
});

test('an extension colliding with the model tag spelling survives in both directions', () => {
  for (const tagged of [true, false]) {
    const wire = {
      ...(tagged ? { tag_kind: 'open' } : {}),
      tagKind: 'extension',
      value_key: 'a',
    };
    for (const sdk of [client, strict]) {
      const model = sdk.openChoiceFromJSON(wire);
      assert.equal(Object.hasOwn(model, 'tagKind'), tagged);
      if (tagged) assert.equal(model.tagKind, 'open');
      assert.equal(model.valueKey, 'a');
      assert.deepEqual(sdk.openChoiceToJSON(model), wire);
    }
    assert.equal(preserve.isOpen(wire), true);
    assert.equal(raw.openChoiceFromJSON(wire), wire);
    assert.equal(raw.openChoiceToJSON(wire), wire);
  }
});

test('an empty typed dictionary is a fallback rather than a strict ambiguity', () => {
  const selected = strict.mapChoiceFromJSON({});
  // The typed member's converter materializes its optional property; the
  // dictionary's identity codec would not add it.
  assert.deepEqual(selected, { payloadValue: undefined });
  assert.equal(JSON.stringify(strict.mapChoiceToJSON(selected)), '{}');
  assert.throws(() => strict.mapChoiceFromJSON({ count: 1 }), /Ambiguous MapChoice tagged union/);
  const wire = { payload_value: 'a' };
  assert.deepEqual(client.mapChoiceFromJSON(wire), { payloadValue: 'a' });
  assert.deepEqual(client.mapChoiceToJSON({ payloadValue: 'a' }), wire);
});

test('a selected inner union receives and preserves the same authored tag', () => {
  for (const sdk of [client, strict]) {
    for (const wire of [
      { kind: 'inner_alt', a_value: 'a' },
      { kind: 'inner_b', b_value: 'b' },
    ]) {
      const value = sdk.outerFromJSON(wire);
      assert.deepEqual(sdk.outerToJSON(value), wire);
    }
    assert.throws(() => sdk.outerFromJSON({ kind: 'inner_b', a_value: 'not b' }), /Unknown variant/);
  }
});

test('one or several composed readers retain a single wire tag', () => {
  for (const kind of ['alpha', 'alpha_alt']) {
    const wire = { kind, label: 'a', count: 3 };
    const model = composedTaggedFromJSON(wire);
    assert.equal(model.kind, kind);
    assert.deepEqual(composedTaggedToJSON(model), wire);
  }
});

test('guards distinguish same-label branches and preserve an existing suffixed name', () => {
  assert.equal(client.isTwin({ kind: 'twin', firstValue: 'a' }), true);
  assert.equal(client.isTwin({ kind: 'twin', secondValue: 'b' }), false);
  assert.equal(client.isTwin3({ kind: 'twin', secondValue: 'b' }), true);
  assert.equal(client.isTwin2({ kind: 'twin_2', thirdValue: 'c' }), true);
  for (const sdk of [preserve, raw]) {
    assert.equal(sdk.isTwin({ kind: 'twin', first_value: 'a' }), true);
    assert.equal(sdk.isTwin3({ kind: 'twin', second_value: 'b' }), true);
    assert.equal(sdk.isTwin2({ kind: 'twin_2', third_value: 'c' }), true);
  }
  const ambiguous = { kind: 'twin', first_value: 'a', second_value: 'b' };
  assert.throws(() => strict.guardChoiceFromJSON(ambiguous), /Ambiguous GuardChoice tagged union/);
});

test('root fields required by different sibling alternatives remain independently present', () => {
  for (const sdk of [client, strict]) {
    for (const [wire, model] of [
      [{ scope_left: { nested_key: 1 } }, { scopeLeft: { nested_key: 1 } }],
      [{ scope_right: 'b' }, { scopeRight: 'b' }],
    ]) {
      const tag = { kind: 'fork_a', label: 'a' };
      assert.deepEqual(sdk.forkFromJSON({ ...tag, ...wire }), { ...tag, ...model });
      assert.deepEqual(sdk.forkToJSON({ ...tag, ...model }), { ...tag, ...wire });
    }
    assert.throws(() => sdk.forkToJSON({ kind: 'fork_a', label: 'a' }), /Unknown variant/);
  }
});
