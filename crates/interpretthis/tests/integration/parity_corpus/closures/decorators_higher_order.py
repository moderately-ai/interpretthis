import functools
def memoize(f):
    cache = {}
    @functools.wraps(f)
    def wrapper(*args):
        if args not in cache:
            cache[args] = f(*args)
        return cache[args]
    return wrapper
@memoize
def fib(n):
    return n if n < 2 else fib(n-1) + fib(n-2)
print(fib(20), fib.__name__)
def curry(f):
    @functools.wraps(f)
    def curried(*args):
        if len(args) >= 3:
            return f(*args)
        return lambda *more: curried(*(args + more))
    return curried
@curry
def add3(a, b, c):
    return a + b + c
print(add3(1)(2)(3), add3(1, 2)(3), add3(1, 2, 3))
def compose(*funcs):
    def composed(x):
        for f in reversed(funcs):
            x = f(x)
        return x
    return composed
print(compose(lambda x: x+1, lambda x: x*2)(10))
def make_multiplier(n):
    return lambda x: x * n
print([make_multiplier(i)(10) for i in range(1, 4)])
