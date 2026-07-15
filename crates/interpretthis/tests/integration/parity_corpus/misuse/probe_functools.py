import functools
@functools.lru_cache(maxsize=None)
def fib(n):
    return n if n < 2 else fib(n-1) + fib(n-2)
print(fib(20))
print(functools.reduce(lambda a,b: a+b, [1,2,3,4], 0))
print(functools.reduce(lambda a,b: a*b, [1,2,3,4]))
add5 = functools.partial(lambda x,y: x+y, 5)
print(add5(10))
@functools.total_ordering
class Num:
    def __init__(self, v): self.v = v
    def __eq__(self, o): return self.v == o.v
    def __lt__(self, o): return self.v < o.v
print(Num(1) < Num(2), Num(3) >= Num(2), Num(2) <= Num(2))
def greet(name): return f"Hi {name}"
wrapped = functools.partial(greet, name="Bob")
print(wrapped())
print(functools.reduce(lambda a,b: a+b, "abc"))
