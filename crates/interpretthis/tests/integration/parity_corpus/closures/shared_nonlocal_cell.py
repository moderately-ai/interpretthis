def counter():
    count = 0
    def increment():
        nonlocal count
        count += 1
        return count
    def decrement():
        nonlocal count
        count -= 1
        return count
    def get():
        return count
    return increment, decrement, get
inc, dec, get = counter()
print(inc(), inc(), inc(), dec(), get())
funcs = [lambda x, n=i: x + n for i in range(3)]
print([f(10) for f in funcs])
def outer():
    x = 1
    def inner():
        nonlocal x
        x += 10
        return x
    return inner
f = outer()
print(f(), f(), f())
def two_writers():
    x = 0
    def a():
        nonlocal x
        x += 10
        return x
    def b():
        nonlocal x
        x += 100
        return x
    return a, b
a, b = two_writers()
print(a(), b(), a())
def reader_writer():
    val = 5
    def w(n):
        nonlocal val
        val = n
    def r():
        return val
    return w, r
w, r = reader_writer()
print(r())
w(42)
print(r())
