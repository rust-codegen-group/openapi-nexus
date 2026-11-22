import type { Choice, Tree, Tree$Wire } from './generated/inferred-union-selection/dist/index.mjs';
import type { Union as RawUnion } from './generated/inferred-union-raw/dist/index.mjs';
import type { Choice as PreserveChoice } from './generated/inferred-union-preserve/dist/index.mjs';

const nullable: Choice = { kind: null, label: 'gamma' };
const preserved: PreserveChoice = { kind: null, label: 'gamma' };
const leaf: Tree = { kind: 'leaf', label: 'last', count: 2 };
const wire: Tree$Wire = { kind: 'node', children: { first_child: leaf } };
const raw: RawUnion = { kind: 'variant_b_2', payload_b: 4 };
// @ts-expect-error Raw exposes the original spelling rather than a camelCase alias.
const notRaw: RawUnion = { kind: 'variant_b_2', payloadB: 4 };

void [nullable, preserved, leaf, wire, raw, notRaw];
