def f(x=[1, 2, 3][0]):
    return x
print(f())
print(f(99))
CONFIG = {"limit": 10}
def g(limit=CONFIG["limit"]):
    return limit * 2
print(g())
print(g(5))
def h(fn=lambda a: a + 1):
    return fn(10)
print(h())
def j(s=f"{1 + 2}"):
    return s
print(j())
def k(v=3 if True else 4):
    return v
print(k())
def m(items=list(range(3))):
    return items
print(m())
def n(x=len("hello")):
    return x
print(n())
def combined(a=2**3, b={"x": 1}.get("x")):
    return a + b
print(combined())
def comp(x=[i for i in range(3)]):
    return x
print(comp())
def gcomp(g=list(i*2 for i in range(3))):
    return g
print(gcomp())
def dcomp(d={k: k*2 for k in range(3)}):
    return d
print(dcomp())
