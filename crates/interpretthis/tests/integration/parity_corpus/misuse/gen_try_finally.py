def h():
    try:
        yield "a"
    finally:
        print("h-cleanup")
it = h()
print(next(it))
print("between")
print(list(it))
def resource():
    print("open")
    try:
        yield 42
    finally:
        print("close")
for v in resource():
    print("using", v)
def caught():
    try:
        yield 1
    except ValueError:
        print("caught in gen")
        yield 99
g = caught()
print(next(g))
print(g.throw(ValueError))
