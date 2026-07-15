def deco(f):
    def wrapper(*args, **kw):
        return f(*args, **kw) * 2
    return wrapper
@deco
def add(a, b):
    return a + b
print(add(3, 4))
def repeat(n):
    def d(f):
        def w(*a):
            return [f(*a) for _ in range(n)]
        return w
    return d
@repeat(3)
def greet(name):
    return f"hi {name}"
print(greet("x"))
