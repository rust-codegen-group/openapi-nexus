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

const detailValue = { wireName: 'Ada' };
const detailWire = { wire_name: 'Ada' };

for (const [policy, generated] of [['default', client], ['strict', strict]]) {
  test(`${policy}: array-valued object members compile and convert in both directions`, () => {
    assert.deepEqual(generated.arrayPayloadOrBooleanFromJSON({ wire_items: ['one'] }), { wireItems: ['one'] });
    assert.deepEqual(generated.arrayPayloadOrBooleanToJSON({ wireItems: ['one'] }), { wire_items: ['one'] });
  });
}

for (const [policy, generated] of [['default', client], ['strict', strict]]) {
  test(`${policy}: allOf fallback aliases do not shadow a concrete outer member`, () => {
    assert.deepEqual(generated.aliasOuterFromJSON(searchWire), searchValue);
    assert.deepEqual(generated.aliasOuterToJSON(searchValue), searchWire);
    assert.deepEqual(generated.aliasOuterFromJSON(detailWire), detailValue);
    assert.deepEqual(generated.aliasOuterToJSON(detailValue), detailWire);
    for (const unmatched of [null, 'opaque', 7, ['opaque'], { arbitrary_key: true }]) {
      assert.deepEqual(generated.aliasOuterFromJSON(unmatched), unmatched);
      assert.deepEqual(generated.aliasOuterToJSON(unmatched), unmatched);
    }
  });

  test(`${policy}: nullable raw maps remain fallbacks instead of concrete ambiguity`, () => {
    assert.deepEqual(generated.nullableMapFallbackFromJSON(detailWire), detailValue);
    assert.deepEqual(generated.nullableMapFallbackToJSON(detailValue), detailWire);
    assert.equal(generated.nullableMapFallbackFromJSON(null), null);
    assert.equal(generated.nullableMapFallbackToJSON(null), null);
    const unmatched = { arbitrary_key: [1, null, { keep_name: true }] };
    assert.deepEqual(generated.nullableMapFallbackFromJSON(unmatched), unmatched);
    assert.deepEqual(generated.nullableMapFallbackToJSON(unmatched), unmatched);
  });

  test(`${policy}: named inner unions retain conversion through an unrestricted alternative`, () => {
    assert.deepEqual(generated.openInnerFromJSON(detailWire), detailValue);
    assert.deepEqual(generated.openInnerToJSON(detailValue), detailWire);
    assert.deepEqual(generated.outerFromJSON(detailWire), detailValue);
    assert.deepEqual(generated.outerToJSON(detailValue), detailWire);
    const unmatched = { arbitrary_key: [7, null, 'opaque'] };
    assert.equal(generated.outerFromJSON(unmatched), unmatched);
    assert.equal(generated.outerToJSON(unmatched), unmatched);
  });

  test(`${policy}: adjacent tagged unit and payload variants match their direct converters`, () => {
    for (const [wire, value] of [
      [{ event_type: 'empty' }, { eventType: 'empty' }],
      [{ event_type: 'data', event_data: detailWire }, { eventType: 'data', eventData: detailValue }],
    ]) {
      assert.deepEqual(generated.adjacentFromJSON(wire), value);
      assert.deepEqual(generated.adjacentToJSON(value), wire);
      assert.deepEqual(generated.adjacentOrNumberFromJSON(wire), value);
      assert.deepEqual(generated.adjacentOrNumberToJSON(value), wire);
    }
    assert.equal(generated.adjacentOrNumberFromJSON(7), 7);
  });

  for (const binary of [new Blob(['x']), new File(['y'], 'sample.txt')]) {
    test(`${policy}: a ${binary.constructor.name} member uses its actual TypeScript representation`, () => {
      const value = { binaryData: binary, fieldName: 'name' };
      const wire = { binary_data: binary, field_name: 'name' };
      assert.deepEqual(generated.binaryModelToJSON(value), wire);
      assert.deepEqual(generated.binaryUnionToJSON(value), wire);
      assert.deepEqual(generated.binaryUnionFromJSON(wire), value);
    });
  }
}

test('outer strict unions retain inner ambiguity errors in both directions', () => {
  assert.throws(() => strict.ambiguousInnerFromJSON(detailWire), /Ambiguous/);
  assert.throws(() => strict.ambiguousInnerToJSON(detailValue), /Ambiguous/);
  assert.throws(() => strict.ambiguousOuterFromJSON(detailWire), /Ambiguous/);
  assert.throws(() => strict.ambiguousOuterToJSON(detailValue), /Ambiguous/);
  assert.deepEqual(client.ambiguousOuterFromJSON(detailWire), detailValue);
  assert.deepEqual(client.ambiguousOuterToJSON(detailValue), detailWire);
});

test('raw policy keeps nested, nullable, tagged, and binary union values unchanged', () => {
  for (const [base, wire] of [
    ['outer', detailWire], ['aliasOuter', searchWire],
    ['nullableMapFallback', detailWire], ['ambiguousOuter', detailWire],
    ['adjacentOrNumber', { event_type: 'empty' }],
    ['binaryUnion', { binary_data: new Blob(['x']), field_name: 'name' }],
  ]) {
    assert.equal(raw[`${base}FromJSON`](wire), wire);
    assert.equal(raw[`${base}ToJSON`](wire), wire);
  }
});

for (const [policy, generated] of [['default', client], ['strict', strict]]) {
  test(`${policy}: single-member allOf preserves nullable map values`, () => {
    for (const base of ['mapWrapped', 'wrappedMapOrBoolean']) {
      assert.equal(generated[`${base}FromJSON`](null), null);
      assert.equal(generated[`${base}ToJSON`](null), null);
      assert.deepEqual(generated[`${base}FromJSON`](detailWire), detailValue);
      assert.deepEqual(generated[`${base}ToJSON`](detailValue), detailWire);
    }
  });

  test(`${policy}: single-member allOf preserves arrays and converts their elements`, () => {
    for (const base of ['arrayWrapped', 'wrappedArrayOrBoolean']) {
      assert.deepEqual(generated[`${base}FromJSON`]([detailWire]), [detailValue]);
      assert.deepEqual(generated[`${base}ToJSON`]([detailValue]), [detailWire]);
    }
  });

  test(`${policy}: open dictionary values retain concrete nested conversion before raw fallback`, () => {
    const value = { concrete_key: detailValue, opaque_key: { keep_name: true }, string_key: 'opaque' };
    const wire = { concrete_key: detailWire, opaque_key: { keep_name: true }, string_key: 'opaque' };
    assert.deepEqual(generated.openMapFromJSON(wire), value);
    assert.deepEqual(generated.openMapToJSON(value), wire);
    assert.deepEqual(generated.mapOuterFromJSON(wire), value);
    assert.deepEqual(generated.mapOuterToJSON(value), wire);
    for (const opaque of [{}, { opaque_key: { keep_name: true } }]) {
      assert.deepEqual(generated.mapOuterFromJSON(opaque), opaque);
      assert.deepEqual(generated.mapOuterToJSON(opaque), opaque);
    }
  });

  test(`${policy}: inline array unions convert as their named-union control`, () => {
    const value = [{ keep_key: detailValue }, 'opaque'];
    const wire = [{ keep_key: detailWire }, 'opaque'];
    assert.deepEqual(generated.namedContainerFromJSON(wire), value);
    assert.deepEqual(generated.namedContainerToJSON(value), wire);
    assert.deepEqual(generated.inlineMixedValuesFromJSON(wire), value);
    assert.deepEqual(generated.inlineMixedValuesToJSON(value), wire);
  });

  test(`${policy}: inline dictionary unions preserve keys and convert nested values`, () => {
    const value = { object_key: { keep_key: detailValue }, string_key: 'opaque' };
    const wire = { object_key: { keep_key: detailWire }, string_key: 'opaque' };
    assert.deepEqual(generated.inlineDictionaryValuesFromJSON(wire), value);
    assert.deepEqual(generated.inlineDictionaryValuesToJSON(value), wire);
  });
}

test('strict: open dictionary selection retains inner ambiguity errors', () => {
  for (const [suffix, input] of [['FromJSON', detailWire], ['ToJSON', detailValue]]) {
    assert.throws(() => strict[`ambiguousMap${suffix}`]({ keep_key: input }), /Ambiguous/);
    assert.throws(() => strict[`ambiguousMapOuter${suffix}`]({ keep_key: input }), /Ambiguous/);
  }
});

test('inline singleton container unions retain their configured ambiguity policy', () => {
  const value = [{ keep_key: detailValue }, 7];
  const wire = [{ keep_key: detailWire }, 7];
  assert.deepEqual(client.inlineNumericValuesFromJSON(wire), value);
  assert.deepEqual(client.inlineNumericValuesToJSON(value), wire);
  assert.throws(() => strict.inlineNumericValuesFromJSON(wire), /Ambiguous/);
  assert.throws(() => strict.inlineNumericValuesToJSON(value), /Ambiguous/);
  assert.equal(raw.inlineNumericValuesFromJSON(wire), wire);
  assert.equal(raw.inlineNumericValuesToJSON(wire), wire);
});

test('raw: container unions preserve their wire values without nested conversion', () => {
  for (const [base, wire] of [
    ['inlineMixedValues', [{ keep_key: detailWire }, 'opaque']],
    ['inlineDictionaryValues', { keep_key: { nested_key: detailWire } }],
    ['mapOuter', { keep_key: detailWire }],
    ['ambiguousMapOuter', { keep_key: detailWire }],
    ['wrappedMapOrBoolean', null],
    ['wrappedArrayOrBoolean', [detailWire]],
  ]) {
    assert.equal(raw[`${base}FromJSON`](wire), wire);
    assert.equal(raw[`${base}ToJSON`](wire), wire);
  }
});

for (const [policy, generated] of [['default', client], ['strict', strict], ['raw', raw]]) {
  test(`${policy}: HTTP nested containers use the same value conversion as models`, async () => {
    const wire = [[clientWire, searchWire]];
    const value = policy === 'raw' ? wire : [[clientValue, searchValue]];
    let sent;
    const api = new generated.ConversionApi(new generated.Configuration({
      fetchApi: async (_url, init) => {
        sent = JSON.parse(init.body);
        return new Response(JSON.stringify(wire), { status: 200, headers: { 'Content-Type': 'application/json' } });
      },
    }));
    assert.deepEqual(await api.exchangeMatrix({ body: value }), value);
    assert.deepEqual(sent, wire);
  });

  test(`${policy}: lazy HTTP errors use the same nested conversion`, async () => {
    const wire = [[clientWire]];
    const value = policy === 'raw' ? wire : [[clientValue]];
    const api = new generated.ConversionApi(new generated.Configuration({
      fetchApi: async () => new Response(JSON.stringify(wire), { status: 400, headers: { 'Content-Type': 'application/json' } }),
    }));
    let caught;
    try {
      await api.exchangeMatrix({ body: value });
    } catch (error) {
      caught = error;
    }
    assert.ok(caught instanceof generated.ExchangeMatrixError);
    assert.deepEqual(await caught.detail.value(), value);
  });

  test(`${policy}: required nullable HTTP containers distinguish null from a missing body`, async () => {
    for (const input of [null, [clientWire]]) {
      const value = input === null || policy === 'raw' ? input : [clientValue];
      let sent;
      const api = new generated.ConversionApi(new generated.Configuration({
        fetchApi: async (_url, init) => {
          sent = JSON.parse(init.body);
          return new Response(JSON.stringify(input), { status: 200, headers: { 'Content-Type': 'application/json' } });
        },
      }));
      assert.deepEqual(await api.exchangeNullable({ body: value }), value);
      assert.deepEqual(sent, input);
      await assert.rejects(api.exchangeNullable({}), generated.RequiredError);
    }
  });
}
