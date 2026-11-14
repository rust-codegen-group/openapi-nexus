import type { Variant, Variant$Wire } from './generated/tagged-allof/dist/index.mjs';
import type { Variant as PreservedVariant } from './generated/tagged-allof-preserve/dist/index.mjs';
import { isVariantA, isVariantB } from './generated/tagged-allof/dist/index.mjs';
import type { Overlap, Overlap$Wire, OverlapConfig } from './generated/overlapping-allof/dist/index.mjs';

const first: Variant = { type: 'variant_a', label: 'item', arguments: {}, strictMode: false, deferred: false };
const firstWire: Variant$Wire = { type: 'variant_a', label: 'item', arguments: {}, strict_mode: false, deferred: false };
const second: Variant = { type: 'variant_b', label: 'group', members: [first] };
const secondWire: Variant$Wire = { type: 'variant_b', label: 'group', members: [firstWire] };
const third: Variant = { type: 'variant_c', configuration: { type: 'nested', location: '/root', entries: [{ entryPath: '/entry' }] } };
const thirdWire: Variant$Wire = { type: 'variant_c', configuration: { type: 'nested', location: '/root', entries: [{ entry_path: '/entry' }] } };
const preserved: PreservedVariant = firstWire;
const preservedSecond: PreservedVariant = secondWire;
const preservedThird: PreservedVariant = thirdWire;
const inline: Variant = { type: 'variant_e', inlineLabel: 'kept' };
const multiple: Variant = { type: 'variant_f', displayName: 'kept', deferred: false };
const outer: Variant = { type: 'variant_g', outerLabel: 'kept', outerCount: 0 };
const variantD: Variant = { type: 'variant_d', inputShape: { keep_key: true } };
const overlap: Variant = { type: 'variant_j', data: { firstValue: 'one', secondValue: 'two' } };
const overlapWire: Variant$Wire = { type: 'variant_j', data: { first_value: 'one', second_value: 'two' } };
const required: Variant = { type: 'variant_i', label: 'required' };
const nested: Variant = { type: 'variant_k', innerLabel: 'in', outerLabel: 'out' };
const recursive: Overlap = { config: { firstName: 'one', lastName: 'two', nextNode: null } };
const recursiveWire: Overlap$Wire = { config: { first_name: 'one', last_name: 'two', next_node: null } };
const authored: OverlapConfig = { authoredField: true };

function expectType<T>(_value: T): void {}
if (isVariantA(first)) {
  expectType<string>(first.label);
  expectType<boolean | undefined>(first.strictMode);
  expectType<Readonly<Record<string, unknown>>>(first.arguments);
}
if (isVariantB(second)) {
  expectType<readonly Variant[]>(second.members);
}

// @ts-expect-error Variant A labels remain required after allOf composition.
const missingLabel: Variant = { type: 'variant_a', arguments: {} };
// @ts-expect-error Variant B declarations retain their required nested members.
const missingMembers: Variant = { type: 'variant_b', label: 'group' };
// @ts-expect-error Variant C retains its required nested configuration.
const missingConfiguration: Variant = { type: 'variant_c' };
// @ts-expect-error Common options must retain their ergonomic spelling.
const invalidErgonomic: Variant = { type: 'variant_a', label: 'item', arguments: {}, allowed_sources: [] };
// @ts-expect-error Composed wire payloads keep the nested wire field spelling.
const invalidWire: Variant$Wire = { type: 'variant_d', inputShape: {} };
// @ts-expect-error Preserve-mode payloads do not acquire ergonomic field names.
const invalidPreserved: PreservedVariant = { type: 'variant_e', inlineLabel: 'wrong' };
// @ts-expect-error Tagged allOf nested arrays retain their readonly contract.
second.members[0] = first;
// @ts-expect-error Outer requiredness strengthens fields inherited from a reference.
const missingOuterRequired: Variant = { type: 'variant_i' };
// @ts-expect-error The wire declaration must retain the same requiredness.
const missingOuterRequiredWire: Variant$Wire = { type: 'variant_i' };
// @ts-expect-error Both nested allOf object constraints remain required.
const missingOverlapField: Variant = { type: 'variant_j', data: { firstValue: 'one' } };
// @ts-expect-error The merged recursive object retains both constraints.
const missingRecursiveField: Overlap = { config: { firstName: 'one' } };
// @ts-expect-error The recursive wire view does not accept public spellings.
const staleRecursiveKey: Overlap$Wire = { config: { first_name: 'one', lastName: 'two' } };

void [first, firstWire, second, secondWire, third, thirdWire, preserved, preservedSecond, preservedThird,
  inline, multiple, outer, variantD, missingLabel, missingMembers, missingConfiguration,
  invalidErgonomic, invalidWire, invalidPreserved, overlap, overlapWire, required,
  nested, missingOuterRequired, missingOuterRequiredWire, missingOverlapField,
  recursive, recursiveWire, authored, missingRecursiveField, staleRecursiveKey];
