# An infinite `while True: yield` inside a `try` must suspend at the yield
# (the classic cleanup-generator), not run the loop to the iteration cap.
def g():
    try:
        while True:
            x = yield
            print("got", x)
    finally:
        print("cleanup")


gen = g()
next(gen)
gen.send("a")
gen.send("b")
gen.close()
print("---")


def cleanup_gen():
    resources = []
    try:
        while True:
            r = yield len(resources)
            resources.append(r)
    finally:
        print(f"cleanup {len(resources)} resources")


cg = cleanup_gen()
print(next(cg))
print(cg.send("a"))
print(cg.send("b"))
cg.close()
print("===")


# try with else + finally around an infinite yielding while.
def averager():
    total = 0
    count = 0
    try:
        while True:
            v = yield (total / count if count else 0)
            total += v
            count += 1
    finally:
        print("done", count)


a = averager()
next(a)
print(a.send(10))
print(a.send(20))
a.close()

# A bounded for inside try still works (eager path) with finally.
def bounded():
    try:
        for i in range(3):
            yield i
    finally:
        print("for finally")


print(list(bounded()))
