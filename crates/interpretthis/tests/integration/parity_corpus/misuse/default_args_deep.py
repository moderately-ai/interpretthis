def greet(name, greeting="Hello", punctuation="!"):
    return f"{greeting}, {name}{punctuation}"
print(greet("Alice"))
print(greet("Bob", "Hi"))
print(greet("Charlie", punctuation="?"))
print(greet("Dave", greeting="Hey", punctuation="."))
def variadic(*args, **kwargs):
    return f"args={args}, kwargs={kwargs}"
print(variadic(1, 2, 3))
print(variadic(a=1, b=2))
print(variadic(1, 2, x=3, y=4))
def mixed(a, b=2, *args, c=3, **kwargs):
    return (a, b, args, c, kwargs)
print(mixed(1))
print(mixed(1, 20, 30, 40, c=50, d=60))
def kwonly(*, x, y=10):
    return x + y
print(kwonly(x=5))
print(kwonly(x=5, y=20))
def posonly(a, b, /):
    return a - b
print(posonly(10, 3))
