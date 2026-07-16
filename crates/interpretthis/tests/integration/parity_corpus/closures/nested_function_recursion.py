def outer():
    def inner(n):
        if n <= 0:
            return 0
        return n + inner(n - 1)
    return inner
print(outer()(5))
def make():
    def fact(n):
        return 1 if n == 0 else fact(n-1) * n
    return fact
print(make()(5))
import functools
def curry(f):
    def curried(*args):
        if len(args) >= 3:
            return f(*args)
        return lambda *more: curried(*(args + more))
    return curried
@curry
def add3(a, b, c):
    return a + b + c
print(add3(1)(2)(3), add3(1, 2)(3), add3(1, 2, 3))
def counter_maker():
    count = 0
    def helper(n):
        if n == 0:
            return count
        return helper(n - 1)
    return helper
print(counter_maker()(3))
def build_tree(depth):
    def node(d):
        if d == 0:
            return "leaf"
        return ["branch", node(d-1), node(d-1)]
    return node(depth)
print(build_tree(2))
