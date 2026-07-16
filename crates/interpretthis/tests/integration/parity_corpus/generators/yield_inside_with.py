class CM:
    def __init__(self, name):
        self.name = name
    def __enter__(self):
        print(f"enter {self.name}")
        return self.name
    def __exit__(self, *a):
        print(f"exit {self.name}")
        return False
def multi_yield():
    with CM("a") as x:
        yield x
        yield x + "2"
        yield x + "3"
g = multi_yield()
print(list(g))
def two_managers():
    with CM("p") as p, CM("q") as q:
        yield p
        yield q
print(list(two_managers()))
def with_before_after():
    yield "before"
    with CM("mid") as m:
        yield m
    yield "after"
print(list(with_before_after()))
def close_runs_exit():
    with CM("closed") as c:
        yield 1
        yield 2
        yield 3
g2 = close_runs_exit()
print(next(g2))
g2.close()
print("closed done")
def loop_with_yield():
    for i in range(2):
        with CM(f"loop{i}") as l:
            yield l
print(list(loop_with_yield()))
from contextlib import contextmanager
@contextmanager
def ctx(name):
    print(f"setup {name}")
    yield name.upper()
    print(f"teardown {name}")
def use_ctx():
    with ctx("resource") as r:
        yield r
        yield r + "!"
print(list(use_ctx()))
def exception_in_with():
    try:
        with CM("exc") as e:
            yield 1
            raise ValueError("boom")
    except ValueError:
        yield "caught"
print(list(exception_in_with()))
def send_into_with():
    with CM("send") as s:
        received = yield "first"
        yield f"got {received}"
g3 = send_into_with()
print(next(g3))
print(g3.send("hello"))
try:
    next(g3)
except StopIteration:
    print("done")
