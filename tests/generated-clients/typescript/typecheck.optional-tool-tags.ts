import type { Tool, Tool$Wire } from './generated/optional-tool-tags/dist/index.mjs';
import { isCustom, isWebSearch20250305 } from './generated/optional-tool-tags/dist/index.mjs';

const omitted: Tool = { name: 'calendar', inputSchema: {} };
const omittedWire: Tool$Wire = { name: 'calendar', input_schema: {} };
const custom: Tool = { type: 'custom', name: 'calendar', inputSchema: {} };
const early: Tool = { type: 'web_search_20250305', name: 'web_search', maxUses: 0 };
const lateWire: Tool$Wire = { type: 'web_search_20260209', name: 'web_search', max_uses: 0 };

if (isCustom(omitted)) {
  omitted.inputSchema;
}
if (isWebSearch20250305(early)) {
  early.maxUses;
}

// @ts-expect-error Arbitrary discriminator values remain invalid.
const invalid: Tool = { type: 'web_search', name: 'web_search' };

void [omitted, omittedWire, custom, early, lateWire, invalid];
