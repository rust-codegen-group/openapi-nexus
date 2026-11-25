import { isTwin, isTwin3, type ScopedChoice, type ScopedChoice$Wire } from './generated/authored-tagged/dist/index.mjs';
import type { ScopedChoice as RawChoice } from './generated/authored-tagged-raw/dist/index.mjs';
import type { ScopedChoice as PreserveChoice } from './generated/authored-tagged-preserve/dist/index.mjs';

const model: ScopedChoice = { scopeKey: 'scope', rootKey: 'root', aValue: 'a' };
const wire: ScopedChoice$Wire = { scope_key: 'scope', root_key: 'root', a_value: 'a' };
const raw: RawChoice = wire;
const preserved: PreserveChoice = wire;
// @ts-expect-error The union-only field remains required in the model.
const missing: ScopedChoice = { scopeKey: 'scope', aValue: 'a' };

const checkGuards = (value: Parameters<typeof isTwin>[0]) => {
  if (isTwin(value)) value.firstValue satisfies string;
  if (isTwin3(value)) value.secondValue satisfies string;
};

void [model, wire, raw, preserved, missing, checkGuards];
