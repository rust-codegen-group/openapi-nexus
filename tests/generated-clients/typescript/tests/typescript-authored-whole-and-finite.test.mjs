import assert from 'node:assert/strict';
import test from 'node:test';
import * as client from '../generated/authored-whole-and-finite/dist/index.mjs';
import * as preserve from '../generated/authored-whole-and-finite-preserve/dist/index.mjs';

test('inferred tagged unions delegate whole string and array alternatives', () => {
  assert.equal(client.choiceFromJSON('legacy'), 'legacy');
  assert.equal(client.choiceToJSON('legacy'), 'legacy');
  assert.equal(client.isLegacyText('legacy'), true);
  assert.equal(preserve.isLegacyText('legacy'), true);
  const wire = [{ item_value: 'a' }];
  const model = [{ itemValue: 'a' }];
  assert.deepEqual(client.choiceFromJSON(wire), model);
  assert.deepEqual(client.choiceToJSON(model), wire);
  assert.equal(client.isItems(model), true);
  assert.equal(preserve.isItems(wire), true);
  assert.equal(preserve.isItems(model), false);
  assert.deepEqual(client.choiceFromJSON({ kind_key: 'record_alt', body_value: 'a' }), {
    kindKey: 'record_alt',
    bodyValue: 'a',
  });
});

test('a single authored branch can contain both whole values and object payloads', () => {
  assert.equal(client.mixedChoiceFromJSON('legacy'), 'legacy');
  assert.equal(client.mixedChoiceToJSON('legacy'), 'legacy');
  assert.equal(client.isMixedPayload('legacy'), true);
  const wire = { item_value: 'a' };
  const model = { itemValue: 'a' };
  assert.deepEqual(client.mixedChoiceFromJSON(wire), model);
  assert.deepEqual(client.mixedChoiceToJSON(model), wire);
  assert.equal(client.isMixedPayload(model), true);
});

test('authored finite objects and arrays are exact, including key and leaf kinds', () => {
  const wire = {
    kind_key: 'finite_alt',
    marker_value: { leaf_key: [1, true] },
    marker_list: [null, { literal_key: false }],
  };
  const model = {
    kindKey: 'finite_alt',
    markerValue: wire.marker_value,
    markerList: wire.marker_list,
  };
  assert.deepEqual(client.finiteChoiceFromJSON(wire), model);
  assert.deepEqual(client.finiteChoiceToJSON(model), wire);
  assert.equal(client.isFinite(model), true);
  assert.equal(preserve.isFinite(wire), true);
  for (const invalid of [
    { leaf_key: [true, true] },
    { leaf_key: [1, true, false] },
    { leaf_key: [1, true], extra: null },
    Object.create({ leaf_key: [1, true] }),
    { leafKey: [1, true] },
  ]) {
    assert.equal(client.isFinite({ ...model, markerValue: invalid }), false);
    assert.throws(() => client.finiteChoiceFromJSON({ ...wire, marker_value: invalid }), /Unknown variant/);
  }
  assert.throws(() => client.finiteChoiceToJSON({ ...model, markerList: [null, { literal_key: 0 }] }), /Unknown variant/);
});

test('a common composite literal observes the selected payload codec’s model keys', () => {
  const wire = { kind_key: 'known_alt', marker: { leaf_value: true }, item_value: 'a' };
  const model = client.typedChoiceFromJSON(wire);
  assert.deepEqual(model, {
    kindKey: 'known_alt',
    marker: { leafValue: true, noteValue: undefined },
    itemValue: 'a',
  });
  assert.deepEqual(JSON.parse(JSON.stringify(client.typedChoiceToJSON(model))), wire);
  assert.equal(client.isKnown(model), true);
  assert.equal(preserve.isKnown(wire), true);
  assert.equal(client.isKnown({ ...model, marker: { leaf_value: true } }), false);
  assert.equal(client.isKnown({ ...model, marker: { leafValue: true, noteValue: null } }), false);
  assert.equal(preserve.isKnown({ ...wire, marker: { leaf_value: true, note_value: undefined } }), false);
});

test('model literal keys follow the selected member of a named nested union', () => {
  for (const [wireMarker, modelMarker] of [
    [{ snake_key: 'b' }, { snake_key: 'b' }],
    [{ guard_value: true, snake_key: 'a' }, { guardValue: true, snakeKey: 'a' }],
  ]) {
    const wire = { kind_key: 'alias_alt', marker: wireMarker, item_value: 'a' };
    const model = { kindKey: 'alias_alt', marker: modelMarker, itemValue: 'a' };
    assert.deepEqual(client.unionChoiceFromJSON(wire), model);
    assert.deepEqual(client.unionChoiceToJSON(model), wire);
    assert.equal(client.isAlias(model), true);
    assert.equal(preserve.isAlias(wire), true);
  }
  const incorrect = { kindKey: 'alias_alt', marker: { snakeKey: 'b' }, itemValue: 'a' };
  assert.equal(client.isAlias(incorrect), false);
  assert.throws(() => client.unionChoiceToJSON(incorrect), /Unknown variant/);
});
