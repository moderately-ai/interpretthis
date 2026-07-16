from typing import List, Dict, Optional, Union, Tuple, Callable, Any, TypeVar, Generic
print(List[int], Dict[str, int])
print(Optional[int], Union[int, str, None])
print(Tuple[int, ...], Callable[[int, str], bool])
print(Any, List, Dict)
T = TypeVar("T")
print(T)
def f(x: int, y: str = "a") -> bool: return True
print(f.__annotations__)
class Box(Generic[T]):
    def __init__(self, v: T): self.item = v
    def get(self) -> T: return self.item
b = Box[int](42)
print(b.get())
print(List[Dict[str, List[int]]])
print(Union[int, str] == Union[str, int])
print(Union[int, str, None] == Union[None, str, int])
print(Union[int, str] == Union[int, str, float])
def g(items: List[int], mapping: Dict[str, int]) -> Optional[str]:
    return None
print(g.__annotations__)
print(Optional[List[int]])
from typing import NamedTuple
class Point(NamedTuple):
    x: int
    y: int = 0
p = Point(1)
print(p, p.x, p.y, p._asdict())
print(Point._fields)
