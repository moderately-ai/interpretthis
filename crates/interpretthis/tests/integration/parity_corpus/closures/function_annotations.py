# A function exposes its def-time annotations as __annotations__ (parameters in
# declaration order, then "return").
def greet(name: str) -> str:
    return f"Hello, {name}"


print(greet.__annotations__)


def func(a: int, b: str = "default") -> bool:
    return True


print(func.__annotations__)


def no_annotations(x, y):
    return x + y


print(no_annotations.__annotations__)


def partial_ann(a: int, b, c: float):
    return a


print(partial_ann.__annotations__)


def only_return() -> None:
    pass


print(only_return.__annotations__)

print(hasattr(greet, "__annotations__"))
print(greet.__annotations__["name"] is str)
print(greet.__annotations__["return"] is str)
