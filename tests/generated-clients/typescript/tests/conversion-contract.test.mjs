import assert from 'node:assert/strict';
import test from 'node:test';
import * as first from '../generated/union-default/dist/index.mjs';
import * as strict from '../generated/union-strict/dist/index.mjs';
import * as raw from '../generated/union-raw/dist/index.mjs';

const value = { wireName: 'Ada' };
const wire = { wire_name: 'Ada' };

for (const [policy, generated] of [['firstMatch', first], ['strict', strict]]) {
  test(`${policy}: moving an inline union through an alias or property preserves conversion`, () => {
    const input = [{ keep_key: wire }, 'opaque'];
    const output = [{ keep_key: value }, 'opaque'];
    for (const base of ['inlineMixedValues', 'aliasChoice', 'valueAlias']) {
      assert.deepEqual(generated[`${base}FromJSON`](input), output);
      assert.deepEqual(generated[`${base}ToJSON`](output), input);
    }
    assert.deepEqual(generated.inlinePropertyFromJSON({ entries: input }), { entries: output });
    assert.deepEqual(generated.inlinePropertyToJSON({ entries: output }), { entries: input });
  });

  test(`${policy}: additional-value shapes distinguish otherwise identical object members`, () => {
    const input = { marker: 'array', extra_key: [wire] };
    const output = { marker: 'array', extra_key: [value] };
    assert.deepEqual(generated.mixedMapBFromJSON(input), output);
    assert.deepEqual(generated.mixedMapBToJSON(output), input);
    assert.deepEqual(generated.mixedMapChoiceFromJSON(input), output);
    assert.deepEqual(generated.mixedMapChoiceToJSON(output), input);
  });

  test(`${policy}: inline numeric enum facts select the unique member without dropping its fields`, () => {
    assert.deepEqual(generated.numericEnumChoiceFromJSON({ revision: 2, second_value: 'two' }), { revision: 2, secondValue: 'two' });
    assert.deepEqual(generated.numericEnumChoiceToJSON({ revision: 2, secondValue: 'two' }), { revision: 2, second_value: 'two' });
  });

  test(`${policy}: finite recursive values retain both directions of member conversion`, () => {
    assert.deepEqual(generated.recursiveListFromJSON([[wire], wire]), [[value], value]);
    assert.deepEqual(generated.recursiveListToJSON([[value], value]), [[wire], wire]);
  });

  test(`${policy}: recursive dictionaries preserve concrete evidence and opaque fallback identity`, () => {
    const input = { keep_key: wire, nested_key: { keep_key: wire } };
    const output = { keep_key: value, nested_key: { keep_key: value } };
    assert.deepEqual(generated.recursiveDictionaryChoiceFromJSON(input), output);
    assert.deepEqual(generated.recursiveDictionaryChoiceToJSON(output), input);
    for (const opaque of [{}, { keep_key: 'opaque' }]) {
      assert.equal(generated.recursiveDictionaryChoiceFromJSON(opaque), opaque);
      assert.equal(generated.recursiveDictionaryChoiceToJSON(opaque), opaque);
    }
  });

  test(`${policy}: direct tagged payloads delegate through the same inline value plan`, () => {
    const input = { event_type: 'data', event_data: [{ keep_key: wire }, 'opaque'] };
    const output = { eventType: 'data', eventData: [{ keep_key: value }, 'opaque'] };
    assert.deepEqual(generated.adjacentContainerFromJSON(input), output);
    assert.deepEqual(generated.adjacentContainerToJSON(output), input);
  });

  test(`${policy}: a tagged model named Record keeps its generated converter contract`, () => {
    assert.deepEqual(generated.recordFromJSON({ event_type: 'data', event_data: wire }), { eventType: 'data', eventData: value });
    assert.deepEqual(generated.recordToJSON({ eventType: 'data', eventData: value }), { event_type: 'data', event_data: wire });
    assert.deepEqual(generated.recordOrBooleanFromJSON({ event_type: 'data', event_data: wire }), { eventType: 'data', eventData: value });
    assert.deepEqual(generated.recordOrBooleanToJSON({ eventType: 'data', eventData: value }), { event_type: 'data', event_data: wire });
  });

  test(`${policy}: external tagged candidates check typed extensions separately from their payload key`, () => {
    const input = { payload_key: wire, extra_key: [wire] };
    const output = { payload_key: value, extra_key: [value] };
    assert.deepEqual(generated.taggedMapBFromJSON(input), output);
    assert.deepEqual(generated.taggedMapBToJSON(output), input);
    assert.deepEqual(generated.taggedMapChoiceFromJSON(input), output);
    assert.deepEqual(generated.taggedMapChoiceToJSON(output), input);
  });

  test(`${policy}: an open external model named Readonly preserves payloads and extensions`, () => {
    const input = { extension_key: wire, extra_key: [wire] };
    const output = { extension_key: value, extra_key: [value] };
    assert.deepEqual(generated.readonlyFromJSON(input), output);
    assert.deepEqual(generated.readonlyToJSON(output), input);
  });

  test(`${policy}: nullability does not broaden an enum that excludes null`, () => {
    const input = { revision: null, second_value: 'two' };
    const output = { revision: null, secondValue: 'two' };
    assert.deepEqual(generated.nullableNumericTwoFromJSON(input), output);
    assert.deepEqual(generated.nullableNumericTwoToJSON(output), input);
    assert.deepEqual(generated.nullableNumericChoiceFromJSON(input), output);
    assert.deepEqual(generated.nullableNumericChoiceToJSON(output), input);
    assert.deepEqual(generated.explicitNullableEnumChoiceFromJSON([null, 1]), [null, 1]);
    assert.deepEqual(generated.explicitNullableEnumChoiceToJSON([null, 1]), [null, 1]);
  });

  test(`${policy}: nullable wrappers retain their outer enum constraint`, () => {
    const input = { revision: null, second_value: 'two' };
    const output = { revision: null, secondValue: 'two' };
    assert.deepEqual(generated.wrapperTwoFromJSON(input), output);
    assert.deepEqual(generated.wrapperTwoToJSON(output), input);
    assert.deepEqual(generated.wrapperChoiceFromJSON(input), output);
    assert.deepEqual(generated.wrapperChoiceToJSON(output), input);
  });
}

for (const suffix of ['FromJSON', 'ToJSON']) {
  test(`strict: nullable properties and containers delegate inner null ambiguity ${suffix}`, () => {
    assert.throws(() => strict[`nullableScalarInner${suffix}`](null), /Ambiguous NullableScalarInner/);
    assert.throws(() => strict[`wrappedScalarInner${suffix}`](null), /Ambiguous NullableScalarInner/);
    const parent = suffix === 'FromJSON' ? { inner_value: null } : { innerValue: null };
    assert.throws(() => strict[`nullableScalarParent${suffix}`](parent), /Ambiguous NullableScalarInner/);
    assert.throws(() => strict[`nullableScalarArray${suffix}`]([null]), /Ambiguous NullableScalarInner/);
    assert.throws(() => strict[`nullableScalarMap${suffix}`]({ keep_key: null }), /Ambiguous NullableScalarInner/);
    assert.deepEqual(strict[`nullableNonNullScalars${suffix}`]([null]), [null]);
    assert.throws(() => strict[`nullableNonNullScalars${suffix}`]([7]), /Ambiguous/);
  });

  test(`strict: explicit null keeps original and nested ambiguity ${suffix}`, () => {
    assert.throws(() => strict[`nullableOverlap${suffix}`](null), /Ambiguous/);
    assert.throws(() => strict[`explicitNullOverlap${suffix}`](null), /Ambiguous/);
    assert.throws(() => strict[`nestedExplicitNullOverlap${suffix}`](null), /Ambiguous/);
  });
}

for (const [policy, generated] of [['firstMatch', first], ['raw', raw]]) {
  test(`${policy}: nullable delegation retains permissive and raw null controls`, () => {
    assert.equal(generated.nullableScalarInnerFromJSON(null), null);
    assert.equal(generated.wrappedScalarInnerToJSON(null), null);
    assert.deepEqual(generated.nullableScalarParentFromJSON({ inner_value: null }), { innerValue: null });
    assert.deepEqual(generated.nullableScalarParentToJSON({ innerValue: null }), { inner_value: null });
    assert.deepEqual(generated.nullableScalarArrayFromJSON([null]), [null]);
    assert.deepEqual(generated.nullableScalarMapToJSON({ keep_key: null }), { keep_key: null });
  });
}

test('strict: primitive-only inline unions check ambiguity in arrays and dictionaries', () => {
  for (const [base, input] of [['primitiveArrayChoice', [7]], ['primitiveMapChoice', { keep_key: 7 }]]) {
    assert.throws(() => strict[`${base}FromJSON`](input), /Ambiguous/);
    assert.throws(() => strict[`${base}ToJSON`](input), /Ambiguous/);
  }
  assert.throws(() => strict.adjacentPrimitiveFromJSON({ event_type: 'data', event_data: [7] }), /Ambiguous/);
  assert.throws(() => strict.adjacentPrimitiveToJSON({ eventType: 'data', eventData: [7] }), /Ambiguous/);
});

test('strict: nullable wrapping is idempotent and never promotes an opaque fallback', () => {
  for (const base of ['onceNullableOuter', 'twiceNullableOuter']) {
    assert.equal(strict[`${base}FromJSON`](null), null);
    assert.equal(strict[`${base}ToJSON`](null), null);
    assert.deepEqual(strict[`${base}FromJSON`](wire), value);
    assert.deepEqual(strict[`${base}ToJSON`](value), wire);
  }
});

test('raw: aliases and inline containers retain wire values and reference identity', () => {
  const input = [{ keep_key: wire }, 'opaque'];
  for (const base of ['inlineMixedValues', 'aliasChoice', 'valueAlias']) {
    assert.equal(raw[`${base}FromJSON`](input), input);
    assert.equal(raw[`${base}ToJSON`](input), input);
  }
});

test('raw: ordinary parents and siblings convert outside the raw union subtree', () => {
  const input = { request_id: 'r1', raw_choice: [{ keep_key: wire }], plain_detail: wire };
  const output = { requestId: 'r1', rawChoice: [{ keep_key: wire }], plainDetail: value };
  assert.deepEqual(raw.rawEnvelopeFromJSON(input), output);
  assert.deepEqual(raw.rawEnvelopeToJSON(output), input);
  const taggedWire = { event_type: 'data', event_data: [{ keep_key: wire }, 'opaque'] };
  const taggedValue = { eventType: 'data', eventData: taggedWire.event_data };
  assert.deepEqual(raw.adjacentContainerFromJSON(taggedWire), taggedValue);
  assert.deepEqual(raw.adjacentContainerToJSON(taggedValue), taggedWire);
});

for (const [policy, generated] of [['firstMatch', first], ['strict', strict], ['raw', raw]]) {
  test(`${policy}: nested array conversion reads frozen inputs without widening declarations`, () => {
    const input = Object.freeze([Object.freeze([Object.freeze({ ...wire })])]);
    const output = Object.freeze([Object.freeze([Object.freeze({ ...value })])]);
    assert.deepEqual(generated.nestedDetailListsFromJSON(input), output);
    assert.deepEqual(generated.nestedDetailListsToJSON(output), input);
  });

  test(`${policy}: implementation helpers are not public barrel exports`, () => {
    assert.deepEqual(Object.keys(generated).filter((key) => /^(convert|matches)\$/.test(key)), []);
  });

  test(`${policy}: multipart JSON values share the model conversion contract`, async () => {
    const input = [{ keep_key: wire }, 'opaque'];
    const payload = policy === 'raw' ? input : [{ keep_key: value }, 'opaque'];
    let sent;
    const api = new generated.ConversionApi(new generated.Configuration({
      fetchApi: async (url, init) => {
        const form = await new Request(url, init).formData();
        assert.equal(form.has('upload_file'), false);
        sent = JSON.parse(form.get('payload_values'));
        return new Response(null, { status: 204 });
      },
    }));
    await api.exchangeMultipart({ body: { payloadValues: payload } });
    assert.deepEqual(sent, input);
  });

  test(`${policy}: multipart uploads retain supplied filenames and exact binary bytes`, async () => {
    const bytes = Uint8Array.of(0, 13, 10, 127, 255);
    const input = [{ keep_key: wire }, 'opaque'];
    const payload = policy === 'raw' ? input : [{ keep_key: value }, 'opaque'];
    let received = false;
    const api = new generated.ConversionApi(new generated.Configuration({
      fetchApi: async (url, init) => {
        const form = await new Request(url, init).formData();
        const file = form.get('upload_file');
        assert.equal(file.name, 'custom-file.bin');
        assert.equal(file.type, 'application/octet-stream');
        assert.deepEqual(new Uint8Array(await file.arrayBuffer()), bytes);
        assert.deepEqual(JSON.parse(form.get('payload_values')), input);
        received = true;
        return new Response(null, { status: 204 });
      },
    }));
    await api.exchangeMultipart({ body: { payloadValues: payload, uploadFile: { data: new Blob([bytes]), filename: 'custom-file.bin' } } });
    assert.equal(received, true);
  });
}
