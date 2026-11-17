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
from inline_default import models as inline_default_models
from inline_raw import models as inline_raw_models
from inline_strict import models as inline_strict_models
from inline_default.models.collision_map_additional_property import CollisionMapAdditionalProperty
from inline_default.models.collision_map_additional_property2 import CollisionMapAdditionalProperty2
from inline_default.models.selection_inline import SelectionInline, selection_inline_from_dict
from inline_default.models.selection_ref import SelectionRef, selection_ref_from_dict


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


def inline_union_extension_types(wire: dict[str, object]) -> None:
    assert_type(inline_default_models.Inline.from_dict(wire).additional_properties, dict[str, int | str])
    assert_type(inline_default_models.InlineOneOf.from_dict(wire).additional_properties, dict[str, int | str])
    assert_type(inline_default_models.Referenced.from_dict(wire).additional_properties, dict[str, int | str])
    assert_type(inline_default_models.Mixed.from_dict(wire).additional_properties, dict[str, int | str])
    assert_type(inline_default_models.NestedMap.from_dict(wire).additional_properties, dict[str, dict[str, int | str]])
    assert_type(inline_default_models.NestedArray.from_dict(wire).additional_properties, dict[str, list[dict[str, int | str]]])
    assert_type(inline_strict_models.Inline.from_dict(wire).additional_properties, dict[str, int | str])
    assert_type(inline_strict_models.InlineOneOf.from_dict(wire).additional_properties, dict[str, int | str])
    assert_type(inline_strict_models.Referenced.from_dict(wire).additional_properties, dict[str, int | str])
    assert_type(inline_strict_models.Mixed.from_dict(wire).additional_properties, dict[str, int | str])
    assert_type(inline_strict_models.NestedMap.from_dict(wire).additional_properties, dict[str, dict[str, int | str]])
    assert_type(inline_strict_models.NestedArray.from_dict(wire).additional_properties, dict[str, list[dict[str, int | str]]])
    assert_type(inline_raw_models.Inline.from_dict(wire).additional_properties, dict[str, object])
    assert_type(inline_raw_models.InlineOneOf.from_dict(wire).additional_properties, dict[str, object])
    assert_type(inline_raw_models.Referenced.from_dict(wire).additional_properties, dict[str, object])
    assert_type(inline_raw_models.Mixed.from_dict(wire).additional_properties, dict[str, object])
    assert_type(inline_raw_models.NestedMap.from_dict(wire).additional_properties, dict[str, dict[str, object]])
    assert_type(inline_raw_models.NestedArray.from_dict(wire).additional_properties, dict[str, list[dict[str, object]]])
    assert_type(inline_default_models.ConstrainedInline.from_dict(wire).additional_properties, dict[str, list[int | str] | None])
    assert_type(inline_default_models.ConstrainedRef.from_dict(wire).additional_properties, dict[str, list[int | str] | None])
    assert_type(inline_default_models.MixedTypeInline.from_dict(wire).additional_properties, dict[str, dict[str, int | str] | str])
    assert_type(inline_default_models.MixedTypeRef.from_dict(wire).additional_properties, dict[str, dict[str, int | str] | str])
    assert_type(inline_strict_models.ConstrainedInline.from_dict(wire).additional_properties, dict[str, list[int | str] | None])
    assert_type(inline_strict_models.ConstrainedRef.from_dict(wire).additional_properties, dict[str, list[int | str] | None])
    assert_type(inline_strict_models.MixedTypeInline.from_dict(wire).additional_properties, dict[str, dict[str, int | str] | str])
    assert_type(inline_strict_models.MixedTypeRef.from_dict(wire).additional_properties, dict[str, dict[str, int | str] | str])
    assert_type(inline_raw_models.MixedTypeInline.from_dict(wire).additional_properties, dict[str, object])
    assert_type(inline_raw_models.MixedTypeRef.from_dict(wire).additional_properties, dict[str, object])
    assert_type(selection_inline_from_dict(wire), SelectionInline)
    assert_type(selection_ref_from_dict(wire), SelectionRef)
    assert_type(inline_default_models.CollisionMap.from_dict(wire).additional_properties, dict[str, int | str])


def collision_helper_type(value: CollisionMapAdditionalProperty2, authored: CollisionMapAdditionalProperty) -> CollisionMapAdditionalProperty2:
    assert_type(authored.marker, str)
    return value
