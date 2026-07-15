
def make_counter():
    count = [0]
    def helper():
        count[0] += 1
        return count[0]
    return helper
c1 = make_counter()
c2 = make_counter()
print(c1(), c1(), c2(), c1(), c2())
def outer():
    def process(x):
        return x * 2
    return process(5)
def another():
    def process(x):
        return x + 100
    return process(5)
print(outer(), another())
funcs = []
for i in range(3):
    def worker(n=i):
        def inner():
            return n * 10
        return inner()
    funcs.append(worker)
print([f() for f in funcs])
