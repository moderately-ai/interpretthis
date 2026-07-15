def send_in_try():
    try:
        x = yield 1
        y = yield x + 1
        yield y + 1
    finally:
        print("done")
g = send_in_try()
print(next(g))
print(g.send(10))
def resource():
    print("acquire")
    try:
        yield "res"
    finally:
        print("release")
r = resource()
print(next(r))
print("main ends")
