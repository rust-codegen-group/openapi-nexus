import assert from 'node:assert/strict';
import test from 'node:test';
import * as first from '../generated/union-default/dist/index.mjs';
import * as strict from '../generated/union-strict/dist/index.mjs';
import * as raw from '../generated/union-raw/dist/index.mjs';

for (const [policy, generated] of [['firstMatch', first], ['strict', strict]]) {
  test(`${policy}: untyped numeric enums do not inherit the legacy string constraint`, () => {
    for (const [wire, value] of [
      [{ revision: 1, first_value: 'one' }, { revision: 1, firstValue: 'one' }],
      [{ revision: 2, second_value: 'two' }, { revision: 2, secondValue: 'two' }],
    ]) {
      assert.deepEqual(generated.untypedNumericChoiceFromJSON(wire), value);
      assert.deepEqual(generated.untypedNumericChoiceToJSON(value), wire);
    }
  });

  test(`${policy}: nested and outer enum constraints retain their intersection`, () => {
    for (const [wire, value] of [
      [{ revision: 1, first_value: 'one' }, { revision: 1, firstValue: 'one' }],
      [{ revision: 2, second_value: 'two' }, { revision: 2, secondValue: 'two' }],
    ]) {
      assert.deepEqual(generated.nestedEnumChoiceFromJSON(wire), value);
      assert.deepEqual(generated.nestedEnumChoiceToJSON(value), wire);
    }
    assert.deepEqual(generated.numericTwoFromJSON({ revision: 2, second_value: 'two' }), { revision: 2, secondValue: 'two' });
    assert.deepEqual(generated.numericTwoToJSON({ revision: 2, secondValue: 'two' }), { revision: 2, second_value: 'two' });
    assert.deepEqual(generated.nestedNullableEnumChoiceFromJSON({ revision: null, first_value: 'one' }), { revision: null, firstValue: 'one' });
    assert.deepEqual(generated.nestedNullableEnumChoiceToJSON({ revision: null, firstValue: 'one' }), { revision: null, first_value: 'one' });
    assert.deepEqual(generated.nestedNullableEnumChoiceFromJSON({ revision: 2, second_value: 'two' }), { revision: 2, secondValue: 'two' });
  });
}

for (const [policy, generated] of [['firstMatch', first], ['strict', strict], ['raw', raw]]) {
  test(`${policy}: extracted enum null permission guards a nonnullable delegate`, () => {
    for (const base of ['nullableEnumParent', 'nullableInlineEnumParent', 'nullablePlainEnumParent']) {
      for (const scalar of [null, 1]) {
        assert.deepEqual(generated[`${base}FromJSON`]({ scalar_value: scalar }), { scalarValue: scalar });
        assert.deepEqual(generated[`${base}ToJSON`]({ scalarValue: scalar }), { scalar_value: scalar });
      }
    }
    if (policy === 'strict') {
      assert.throws(() => generated.alreadyNullableEnumParentFromJSON({ scalar_value: null }), /Ambiguous NullableScalarInner/);
      assert.throws(() => generated.alreadyNullableEnumParentToJSON({ scalarValue: null }), /Ambiguous NullableScalarInner/);
    } else {
      assert.deepEqual(generated.alreadyNullableEnumParentFromJSON({ scalar_value: null }), { scalarValue: null });
      assert.deepEqual(generated.alreadyNullableEnumParentToJSON({ scalarValue: null }), { scalar_value: null });
    }
  });
}

for (const direction of ['FromJSON', 'ToJSON']) {
  test(`strict: open nested unions keep authored concrete null ambiguity ${direction}`, () => {
    assert.throws(() => strict[`nullableOpenInner${direction}`](null), /Ambiguous NullableOpenInner/);
    assert.throws(() => strict[`nullableOpenExplicitOuter${direction}`](null), /Ambiguous NullableOpenInner/);
    assert.throws(() => strict[`nullableOpenSiblingOuter${direction}`](null), /Ambiguous NullableOpenSiblingOuter/);
    assert.equal(strict[`nullableOpaqueSiblingOuter${direction}`](null), null);
    for (const value of ['text', 1.5]) {
      assert.equal(strict[`nullableOpenExplicitOuter${direction}`](value), value);
    }
    assert.equal(strict[`nullableOpenSiblingOuter${direction}`](true), true);
    const opaque = { keep_key: 'opaque' };
    assert.equal(strict[`nullableOpenExplicitOuter${direction}`](opaque), opaque);
  });
}

for (const [policy, generated] of [['firstMatch', first], ['raw', raw]]) {
  test(`${policy}: open nullable unions retain permissive and raw controls`, () => {
    for (const base of ['nullableOpenInner', 'nullableOpenExplicitOuter', 'nullableOpenSiblingOuter', 'nullableOpaqueSiblingOuter']) {
      assert.equal(generated[`${base}FromJSON`](null), null);
      assert.equal(generated[`${base}ToJSON`](null), null);
    }
    const input = { revision: 2, second_value: 'two' };
    if (policy === 'raw') {
      assert.equal(generated.nestedEnumChoiceFromJSON(input), input);
      assert.equal(generated.nestedEnumChoiceToJSON(input), input);
    }
  });
}

function http(generated) {
  const sent = [];
  const api = new generated.ConversionApi(new generated.Configuration({
    fetchApi: async (_url, init) => {
      sent.push(init.body);
      return new Response(null, { status: 204 });
    },
  }));
  return { api, sent };
}

for (const [direction, input, expected] of [
  ['FromJSON', { scalar_value: null }, { scalarValue: null }],
  ['ToJSON', { scalarValue: null }, { scalar_value: null }],
]) {
  test(`strict: nested finite boundary retains outer-only null permission ${direction}`, () => {
    assert.deepEqual(strict[`containerOuterEnum${direction}`]([null, 1]), [null, 1]);
    assert.deepEqual(strict[`enumFreeOuterEnumParent${direction}`](input), expected);
    assert.deepEqual(strict[`constrainedOuterEnumParent${direction}`](input), expected);
  });
  test(`strict: nested finite boundary retains inner-owned null ambiguity ${direction}`, () => {
    assert.throws(() => strict[`alreadyNullableOuterEnumParent${direction}`](input), /Ambiguous NullableScalarInner/);
  });
  for (const [policy, generated] of [['firstMatch', first], ['raw', raw]]) {
    test(`${policy}: nested finite null controls remain usable ${direction}`, () => {
      assert.deepEqual(generated[`constrainedOuterEnumParent${direction}`](input), expected);
      assert.deepEqual(generated[`containerOuterEnum${direction}`]([null, 1]), [null, 1]);
    });
  }
}

test('strict: direct and required JSON body retain null ambiguity', async () => {
  assert.throws(() => strict.nullableScalarInnerToJSON(null), /Ambiguous NullableScalarInner/);
  const { api, sent } = http(strict);
  await assert.rejects(api.sendRequiredScalar({ body: null }), /Ambiguous NullableScalarInner/);
  assert.deepEqual(sent, []);
});

for (const method of ['sendOptionalScalar', 'sendOptionalInlineScalar']) {
  test(`strict: ${method} delegates supplied null rather than treating it as omission`, async () => {
    const { api, sent } = http(strict);
    await assert.rejects(api[method]({ body: null }), /Ambiguous/);
    assert.deepEqual(sent, []);
  });
}

for (const [policy, generated] of [['firstMatch', first], ['strict', strict], ['raw', raw]]) {
  test(`${policy}: optional JSON bodies preserve omission, allowed null and converted values`, async () => {
    const { api, sent } = http(generated);
    await api.sendOptionalScalar({});
    await api.sendOptionalInlineScalar({});
    await api.sendOptionalDetail({});
    await api.sendOptionalDetail({ body: null });
    await api.sendOptionalDetail({ body: policy === 'raw' ? { wire_name: 'Ada' } : { wireName: 'Ada' } });
    assert.deepEqual(sent, [undefined, undefined, undefined, 'null', '{"wire_name":"Ada"}']);
    if (policy !== 'strict') {
      await api.sendOptionalScalar({ body: null });
      assert.equal(sent.at(-1), 'null');
    }
  });
}
