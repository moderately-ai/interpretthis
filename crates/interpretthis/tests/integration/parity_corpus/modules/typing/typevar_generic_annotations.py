from typing import List, Dict, Optional, Union, Tuple, Set, Any, Callable
def greet(name: str) -> str:
    return f"Hello {name}"
print(greet("world"))
def process(items: List[int]) -> int:
    return sum(items)
print(process([1, 2, 3]))
def lookup(d: Dict[str, int], key: str) -> Optional[int]:
    return d.get(key)
print(lookup({"a": 1}, "a"), lookup({"a": 1}, "b"))
def combine(x: Union[int, str]) -> str:
    return str(x)
print(combine(5), combine("hi"))
def coords() -> Tuple[int, int]:
    return (1, 2)
print(coords())
x: int = 5
y: List[str] = ["a", "b"]
z: Dict[str, int] = {"key": 1}
print(x, y, z)
from typing import TypeVar, Generic
T = TypeVar("T")
class Stack(Generic[T]):
    def __init__(self):
        self.items: List[T] = []
    def push(self, item: T) -> None:
        self.items.append(item)
    def pop(self) -> T:
        return self.items.pop()
s = Stack()
s.push(1)
s.push(2)
print(s.pop(), s.pop())
from dataclasses import dataclass, field
@dataclass
class Config:
    name: str
    values: List[int] = field(default_factory=list)
    enabled: bool = True
cfg = Config("test")
cfg.values.append(1)
print(cfg.name, cfg.values, cfg.enabled)
print(cfg)
from typing import NamedTuple
class Point(NamedTuple):
    x: int
    y: int = 0
p = Point(1)
print(p.x, p.y)
print(p)
from enum import Enum
class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3
print(Color.RED, Color.RED.value, Color.RED.name)
print(list(Color))
print(Color(2))
print(Color["BLUE"])
handler: Callable[[int], int] = lambda x: x * 2
print(handler(5))
