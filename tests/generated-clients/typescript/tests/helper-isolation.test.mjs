import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import test from 'node:test';

const require = createRequire(import.meta.url);
const client = require('../../../../target/generated-clients/typescript-isolation/isolation-consumer.js');

test('selected model helpers do not load unrelated model graphs', () => {
  const wire = [{ wire_name: 'Ada' }];
  const value = [{ wireName: 'Ada' }];
  assert.deepEqual(client.usefulFromJSON(wire), value);
  assert.deepEqual(client.usefulToJSON(value), wire);
});
