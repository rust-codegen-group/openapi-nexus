"""Public extension types retain declared payloads on both transports."""

from typing import Any, assert_type

from additional_default.apis.default_api import DefaultApi
from additional_default.models import DeepValues, Holder, MixedMap, NestedValue, OpaqueMap, StorageCollision, TypedMap
from additional_default.models.dictionary_choice import dictionary_choice_from_dict
from additional_default.models import DictionaryChoice
from additional_raw.models import Choices as RawChoices
from additional_strict.models import MixedMap as StrictMixedMap
from additional_default.models import Any as OpaqueNamedAny
from additional_default.models import Event, OptionalHolder
from additional_default.models.event import event_from_dict, event_to_dict


def typed_extensions(wire: dict[str, object], member: NestedValue, holder: Holder) -> MixedMap:
    value = MixedMap(known_field="known", additional_properties={"keep_key": member})
    assert_type(value.additional_properties, dict[str, NestedValue])
    assert_type(TypedMap.from_dict(wire).additional_properties, dict[str, NestedValue])
    assert_type(DeepValues.from_dict(wire).additional_properties, dict[str, list[dict[str, NestedValue]]])
    assert_type(StorageCollision.from_dict(wire).additional_properties_, dict[str, NestedValue])
    assert_type(OpaqueMap.from_dict(wire).additional_properties, dict[str, Any])
    assert_type(OpaqueNamedAny.from_dict(wire).additional_properties, dict[str, Any])
    assert_type(holder.aliases, dict[str, TypedMap])
    assert_type(dictionary_choice_from_dict(wire), DictionaryChoice)
    return value


def typed_http(api: DefaultApi, value: MixedMap, dictionary: TypedMap) -> MixedMap:
    assert_type(api.echo_dictionary(body=dictionary), TypedMap)
    assert_type(api.echo_mixed_with_http_info(body=value).data, MixedMap)
    return api.echo_mixed(body=value)


def policy_types(wire: dict[str, object]) -> None:
    assert_type(StrictMixedMap.from_dict(wire).additional_properties["key"].wire_value, str)
    assert_type(RawChoices.from_dict(wire).additional_properties, dict[str, object])


def compatible_helpers(wire: dict[str, object], value: Event) -> OptionalHolder:
    assert_type(event_from_dict(data=wire), Event)
    assert_type(event_to_dict(obj=value), dict[str, object])
    holder = OptionalHolder(maybe=None, nullable=None)
    assert_type(holder.maybe, MixedMap | None)
    assert_type(holder.nullable, MixedMap | None)
    return holder
