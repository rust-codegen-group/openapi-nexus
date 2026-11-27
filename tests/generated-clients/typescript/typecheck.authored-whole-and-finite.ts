import {
  choiceFromJSON,
  choiceToJSON,
  isItems,
  isLegacyText,
  isMixedPayload,
  mixedChoiceFromJSON,
  mixedChoiceToJSON,
  type Choice,
  type Choice$Wire,
  type Item,
  type Items,
  type MixedChoice,
  type MixedChoice$Wire,
} from './generated/authored-whole-and-finite/dist/index.mjs';
import {
  isItems as isPreservedItems,
  isLegacyText as isPreservedLegacyText,
  type Choice as PreserveChoice,
} from './generated/authored-whole-and-finite-preserve/dist/index.mjs';

const text: Choice$Wire = 'legacy';
const wireItems: Choice$Wire = [{ item_value: 'a' }];
const items: Choice = [{ itemValue: 'a' }];
const mixedWire: MixedChoice$Wire = 'legacy';
const mixedModel: MixedChoice = { itemValue: 'a' };
const preserveText: PreserveChoice = 'legacy';
const preserveItems: PreserveChoice = [{ item_value: 'a' }];
// @ts-expect-error A number is not one of these whole values.
const invalid: Choice = 123;
// @ts-expect-error Typed array elements use the original key in the wire type.
const invalidItems: Choice$Wire = [{ itemValue: 'a' }];

function narrow(value: Choice, mixed: MixedChoice, preserved: PreserveChoice) {
  if (isLegacyText(value)) value satisfies string;
  if (isItems(value)) value satisfies Items;
  if (isMixedPayload(mixed)) {
    if (typeof mixed === 'string') mixed satisfies string;
    else mixed satisfies Item;
  }
  if (isPreservedLegacyText(preserved)) preserved satisfies string;
  if (isPreservedItems(preserved)) preserved[0]?.item_value satisfies string | undefined;
}

void [
  choiceFromJSON(text),
  choiceFromJSON(wireItems),
  choiceToJSON(items),
  mixedChoiceFromJSON(mixedWire),
  mixedChoiceToJSON(mixedModel),
  preserveText,
  preserveItems,
  invalid,
  invalidItems,
  narrow,
];
