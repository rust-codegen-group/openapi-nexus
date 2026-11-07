import type {
  AllOfMixedStringMap,
  AllOfMixedStringMap$Wire,
  JsonModelAllOf,
  JsonModelAllOf$Wire,
  AliasMap,
  AliasMap$Wire,
  DetailAliasChain,
  DetailAliasChain$Wire,
  DetailListAlias,
  DetailListAlias$Wire,
  NullableDetailAlias,
  NullableDetailAlias$Wire,
  OpenExternalEvent,
  OpenExternalEvent$Wire,
  TypedExternalEvent,
  TypedExternalEvent$Wire,
  Detail,
  Detail$Wire,
  MixedModelMap,
  MixedModelMap$Wire,
  MixedStringMap,
  MixedStringMap$Wire,
  PlainModelMap,
  PlainModelMap$Wire,
  PlainStringMap,
} from './generated/camel-case/dist/index.mjs';
import type {
  AllOfMixedStringMap as PreservedAllOfMap,
  MixedModelMap as PreservedModelMap,
  MixedStringMap as PreservedStringMap,
} from './generated/preserve/dist/index.mjs';
import type { Tool, Tool$Wire, ToolList, ToolMap, ToolRequest } from './generated/union-default/dist/index.mjs';
import type { Tool as RawTool } from './generated/union-raw/dist/index.mjs';

const unionTool: Tool = { name: 'weather', inputSchema: { type: 'object' } };
const unionWire: Tool$Wire = { name: 'weather', input_schema: { type: 'object' } };
const unionRequest: ToolRequest = { tools: [unionTool] };
const unionList: ToolList = [{ name: 'weather', inputSchema: {} }];
const unionMap: ToolMap = { arbitrary_key: { name: 'weather', inputSchema: {} } };
const rawUnion: RawTool = unionWire;
// @ts-expect-error Normal unions retain the ergonomic member contract.
const invalidUnion: Tool = { name: 'weather', input_schema: {} };
// @ts-expect-error Raw unions expose wire members rather than ergonomic members.
const invalidRawUnion: RawTool = { name: 'weather', inputSchema: {} };
// @ts-expect-error Union array alternatives remain readonly.
unionList[0] = { name: 'changed', inputSchema: {} };

function expectType<T>(_value: T): void {}

declare const allOf: AllOfMixedStringMap;
declare const allOfWire: AllOfMixedStringMap$Wire;
declare const preservedAllOf: PreservedAllOfMap;
expectType<string | undefined>(allOf.known);
expectType<string | undefined>(allOfWire.known);
expectType<string | undefined>(preservedAllOf.known);
// @ts-expect-error Composed ergonomic dictionaries are readonly.
allOf.extension = 'changed';
// @ts-expect-error Composed wire dictionaries are readonly.
allOfWire.extension = 'changed';
// @ts-expect-error Preserve-mode composed dictionaries are readonly.
preservedAllOf.extension = 'changed';

const composedModels: JsonModelAllOf = { knownFirst: { displayName: 'Ada' }, knownSecond: { displayName: 'Grace' }, extra_key: { displayName: 'Lin' } };
const composedModelsWire: JsonModelAllOf$Wire = { known_first: { display_name: 'Ada' }, known_second: { display_name: 'Grace' }, extra_key: { display_name: 'Lin' } };
expectType<Detail>(composedModels.knownFirst);
expectType<Detail$Wire>(composedModelsWire.known_first);
// @ts-expect-error Readonly composition must retain the declared model's precise type.
const invalidComposedModel: JsonModelAllOf = { knownFirst: { display_name: 'Ada' }, knownSecond: { displayName: 'Grace' } };
// @ts-expect-error Composed model extensions are readonly.
composedModels.extra_key = { displayName: 'changed' };
// @ts-expect-error Composed wire model extensions are readonly.
composedModelsWire.extra_key = { display_name: 'changed' };

const plain: PlainStringMap = { extra_dynamic: 'opaque' };
expectType<string>(plain.extra_dynamic);
// @ts-expect-error Additional string values must not silently become untyped.
const invalidPlain: PlainStringMap = { extra_dynamic: 42 };

const mixed: MixedStringMap = { known: 'fixed', extra_dynamic: 'opaque' };
const mixedWire: MixedStringMap$Wire = { known: 'fixed', extra_dynamic: 'opaque' };
expectType<string | undefined>(mixed.extra_dynamic);
expectType<string | undefined>(mixedWire.extra_dynamic);
// @ts-expect-error A known string property retains its declared type.
const invalidKnown: MixedStringMap = { known: 42 };

const models: MixedModelMap = { knownCount: 1, dynamic_key: { displayName: 'Ada' } };
const modelsWire: MixedModelMap$Wire = { known_count: 1, dynamic_key: { display_name: 'Ada' } };
expectType<number>(models.knownCount);
expectType<number>(modelsWire.known_count);
expectType<Detail | number>(models.dynamic_key);
expectType<Detail$Wire | number>(modelsWire.dynamic_key);
// @ts-expect-error Additional models use ergonomic names, not wire names.
const invalidModels: MixedModelMap = { knownCount: 1, dynamic_key: { display_name: 'Ada' } };

const plainModels: PlainModelMap = { dynamic_key: { displayName: 'Ada' } };
const plainModelsWire: PlainModelMap$Wire = { dynamic_key: { display_name: 'Ada' } };
expectType<Detail>(plainModels.dynamic_key);
expectType<Detail$Wire>(plainModelsWire.dynamic_key);

const preserved: PreservedStringMap = { known: 'fixed', extra_dynamic: 'opaque' };
const preservedModels: PreservedModelMap = { known_count: 1, dynamic_key: { display_name: 'Ada' } };
expectType<string | undefined>(preserved.extra_dynamic);
expectType<number>(preservedModels.known_count);

const aliases: AliasMap = { knownField: 'fixed', dynamic_key: { displayName: 'Ada' } };
const aliasesWire: AliasMap$Wire = { known_field: 'fixed', dynamic_key: { display_name: 'Ada' } };
expectType<DetailAliasChain | string>(aliases.dynamic_key);
expectType<DetailAliasChain$Wire | string>(aliasesWire.dynamic_key);
const aliasedList: DetailListAlias = [{ displayName: 'Ada' }];
const aliasedListWire: DetailListAlias$Wire = [{ display_name: 'Ada' }];
expectType<readonly Detail[]>(aliasedList);
expectType<readonly Detail$Wire[]>(aliasedListWire);
const nullableAlias: NullableDetailAlias = { displayName: 'Ada' };
const nullableAliasWire: NullableDetailAlias$Wire = { display_name: 'Ada' };
expectType<DetailAliasChain | null>(nullableAlias);
expectType<DetailAliasChain$Wire | null>(nullableAliasWire);
expectType<NullableDetailAlias>(null);
expectType<NullableDetailAlias$Wire>(null);
// @ts-expect-error Wire dictionary aliases must not accept ergonomic model names.
const invalidAliasesWire: AliasMap$Wire = { known_field: 'fixed', dynamic_key: { displayName: 'Ada' } };
// @ts-expect-error Nullable wire aliases must still use the referenced wire model.
const invalidNullableAliasWire: NullableDetailAlias$Wire = { displayName: 'Ada' };

const external: OpenExternalEvent = { started: { displayName: 'Ada' }, opaque_key: { nested_key: 7 } };
const externalWire: OpenExternalEvent$Wire = { started: { display_name: 'Ada' }, opaque_key: { nested_key: 7 } };
expectType<OpenExternalEvent>(external);
expectType<OpenExternalEvent$Wire>(externalWire);
// @ts-expect-error Open wrapper extensions must not erase the payload's wire type.
const invalidExternalWire: OpenExternalEvent$Wire = { started: { displayName: 'Ada' } };
const typedExternal: TypedExternalEvent = { started: { displayName: 'Ada' }, extra_dynamic: { displayName: 'Grace' } };
const typedExternalWire: TypedExternalEvent$Wire = { started: { display_name: 'Ada' }, extra_dynamic: { display_name: 'Grace' } };
expectType<TypedExternalEvent>(typedExternal);
expectType<TypedExternalEvent$Wire>(typedExternalWire);
