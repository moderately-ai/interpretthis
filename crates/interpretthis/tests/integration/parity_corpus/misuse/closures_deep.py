def counter():
    count = 0
    def inc():
        nonlocal count
        count += 1
        return count
    return inc
c = counter()
print(c(), c(), c())
def make_adders():
    return [lambda x, n=i: x + n for i in range(3)]
adders = make_adders()
print(adders[0](10), adders[1](10), adders[2](10))
def outer():
    x = "outer"
    def middle():
        def inner():
            return x
        return inner()
    return middle()
print(outer())
funcs = []
for i in range(3):
    funcs.append(lambda: i)
print([f() for f in funcs])
