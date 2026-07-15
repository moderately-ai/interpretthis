def double(f):
    def wrapper(*args, **kwargs):
        return f(*args, **kwargs) * 2
    return wrapper
@double
def add(a, b):
    return a + b
print(add(3, 4))
def repeat(n):
    def deco(f):
        def wrapper(*a, **k):
            return [f(*a, **k) for _ in range(n)]
        return wrapper
    return deco
@repeat(3)
def hello():
    return "hi"
print(hello())
import functools
def logged(f):
    @functools.wraps(f)
    def wrapper(*a, **k):
        return f(*a, **k)
    return wrapper
@logged
def greet(name):
    "greet docstring"
    return f"Hello {name}"
print(greet("Bob"))
print(greet.__name__)
@double
@double
def num():
    return 5
print(num())
