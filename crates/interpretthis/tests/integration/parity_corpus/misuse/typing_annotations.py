from typing import List, Dict, Optional, Union, Tuple
def process(items: List[int]) -> Dict[str, int]:
    return {str(i): i for i in items}
print(process([1, 2, 3]))
def maybe(x: Optional[int] = None) -> int:
    return x if x is not None else 0
print(maybe(), maybe(5))
x: int = 10
y: List[str] = ["a", "b"]
print(x, y)
def multi(a: int, b: str, c: float = 1.0) -> Tuple[int, str, float]:
    return (a, b, c)
print(multi(1, "x"))
from typing import Any, Callable
def apply(f: Callable[[int], int], val: int) -> int:
    return f(val)
print(apply(lambda x: x * 2, 5))
