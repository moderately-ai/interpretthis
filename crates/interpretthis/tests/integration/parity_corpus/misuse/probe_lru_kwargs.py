import functools
@functools.lru_cache(maxsize=None)
def f(a, b=10):
    return a + b
print(f(1))
print(f(1, b=20))
print(f(1, 20))
print(f(a=1, b=30))
print(f(1))
ci = f.cache_info()
print(ci.hits, ci.misses, ci.currsize)
print(ci)
calls = []
@functools.lru_cache(maxsize=2)
def g(x, y=0):
    calls.append((x, y))
    return x * 2 + y
print(g(5))
print(g(5))
print(g(5, y=1))
print(len(calls))
print(g.cache_info().maxsize)
g.cache_clear()
print(g.cache_info().hits, g.cache_info().currsize)
