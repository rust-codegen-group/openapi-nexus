"""Strict consumer typing for the authored-tagged model barrels and helpers."""

from typing import assert_type

from authored_echo_default.models import Message, MessageText, MessageTextCase
from authored_echo_default.models.message import message_from_dict, message_to_dict
from authored_echo_raw.models.message import message_from_dict as raw_from_dict
from authored_four.models import PayloadA, Union, UnionVariantACase
from authored_four.models.union import union_from_dict


message = MessageTextCase(MessageText(label="new"), "text_alt")
assert_type(message_from_dict(message_to_dict(message)), Message)
assert_type(raw_from_dict(None), object)

value = union_from_dict({"payload_a": "a", "payload_a_schema": {}})
assert_type(value, Union)
if isinstance(value, UnionVariantACase):
    assert_type(value.payload, PayloadA)
constructed = UnionVariantACase(PayloadA(payload_a="a", payload_a_schema={}))
assert_type(constructed.payload, PayloadA)
