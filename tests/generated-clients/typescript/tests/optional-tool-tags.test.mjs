import assert from 'node:assert/strict';
import test from 'node:test';
import * as client from '../generated/optional-tool-tags/dist/index.mjs';

const json = (value) => JSON.parse(JSON.stringify(value));
const schema = { properties: { keep_key: { type: 'string' } } };

for (const [name, value, wire] of [
  ['omitted custom tag', { name: 'calendar', inputSchema: schema }, { name: 'calendar', input_schema: schema }],
  ['explicit custom tag', { type: 'custom', name: 'calendar', inputSchema: schema }, { type: 'custom', name: 'calendar', input_schema: schema }],
  ...['web_search_20250305', 'web_search_20260209'].map((type) => [
    type,
    { type, name: 'web_search', maxUses: 0 },
    { type, name: 'web_search', max_uses: 0 },
  ]),
]) {
  test(`optional/multi-valued tag ${name}: to wire`, () => {
    assert.deepEqual(json(client.toolToJSON(value)), wire);
  });
  test(`optional/multi-valued tag ${name}: from wire`, () => {
    assert.deepEqual(json(client.toolFromJSON(wire)), value);
  });
}
