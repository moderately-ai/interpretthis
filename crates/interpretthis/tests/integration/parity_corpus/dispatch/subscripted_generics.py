# Pins: PEP 585 subscripted generics in annotations (`list[int]`,
# `dict[str, int]`). The annotation is informational at runtime;
# the value still must be a regular list/dict.
items: list[int] = [1, 2, 3]
print(items)

mapping: dict[str, int] = {'a': 1}
print(mapping)
