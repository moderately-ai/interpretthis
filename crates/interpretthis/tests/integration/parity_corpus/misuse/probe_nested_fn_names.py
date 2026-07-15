def outer1():
    def helper():
        return "one"
    return helper()
def outer2():
    def helper():
        return "two"
    return helper()
print(outer1(), outer2())
def make(n):
    def compute():
        return n * 10
    return compute
a = make(5)
b = make(7)
print(a(), b())
def f1():
    def g():
        return 1
    return g
def f2():
    def g():
        return 2
    return g
print(f1()(), f2()())
