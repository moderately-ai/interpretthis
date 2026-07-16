from typing import List, Dict, Optional, Union, Tuple, Callable, Set, FrozenSet

# A subscripted typing alias reprs as typing.X[...] (CPython's _GenericAlias),
# and the bare alias as typing.X — not the <class '...'> form of a builtin type.
print(List)
print(Dict)
print(List[int])
print(Dict[str, int])
print(Optional[int])
print(Union[int, str])
print(Tuple[int, str])
print(Set[int])
print(FrozenSet[int])
print(List[Dict[str, int]])
print(Tuple[int, ...])
print(Callable[[int], str])
print(List[Tuple[int, str]])


def f(x: List[int], y: Dict[str, int]) -> Optional[int]:
    return None


print(f.__annotations__)


def h(items: List[Tuple[int, str]]) -> Dict[str, List[int]]:
    return {}


print(h.__annotations__)
