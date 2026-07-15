def kwonly(a, *, b, c=10):
    return a + b + c
print(kwonly(1, b=2))
print(kwonly(1, b=2, c=3))
def posonly(a, b, /, c):
    return a + b + c
print(posonly(1, 2, 3))
print(posonly(1, 2, c=3))
def defaults(a, b=2, *args, c=3, **kwargs):
    return (a, b, args, c, kwargs)
print(defaults(1))
print(defaults(1, 2, 3, 4, c=5, d=6))
def annotated(x: int, y: str = "hi") -> bool:
    return len(y) > x
print(annotated(1))
print(annotated(5, "hello world"))
