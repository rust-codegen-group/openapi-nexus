import type { Union, Union$Wire, UnionContainer } from './generated/optional-discriminator-union/dist/index.mjs';
import { isVariantA, isVariantB1, isVariantC, isVariantD } from './generated/optional-discriminator-union/dist/index.mjs';

const omitted: Union = { payloadA: 'alpha', payloadASchema: {} };
const omittedWire: Union$Wire = { payload_a: 'alpha', payload_a_schema: {} };
const explicit: Union = { kind: 'variant_a', payloadA: 'alpha', payloadASchema: {} };
const early: Union = { kind: 'variant_b_1', payloadB: 0 };
const lateWire: Union$Wire = { kind: 'variant_b_2', payload_b: 0 };
const singleton: Union = { kind: 'variant_c', payloadC: 'gamma' };
const variantD: Union = { payloadD: 'delta' };
const nested: UnionContainer = { payloadA: 'alpha', payloadASchema: {} };

if (isVariantA(omitted)) {
  omitted.payloadASchema;
}
if (isVariantA(explicit)) {
  explicit.payloadA;
}

// @ts-expect-error Arbitrary discriminator values remain invalid.
const invalid: Union = { kind: 'variant_unknown', payloadB: 0 };

if (isVariantB1(early)) {
  early.payloadB;
}
if (isVariantC(singleton)) {
  singleton.payloadC;
}
if (isVariantD(variantD)) {
  variantD.payloadD;
}

void [omitted, omittedWire, explicit, early, lateWire, singleton, variantD, nested, invalid];
