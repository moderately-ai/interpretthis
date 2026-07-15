class Ctx:
    def __init__(self, name): self.name = name
    def __enter__(self):
        print(f"enter {self.name}")
        return self
    def __exit__(self, *a):
        print(f"exit {self.name}")
        return False
with Ctx("a") as c:
    print(f"body {c.name}")
with Ctx("x") as x, Ctx("y") as y:
    print("nested body")
from contextlib import contextmanager
@contextmanager
def managed():
    print("setup")
    yield 42
    print("teardown")
with managed() as v:
    print(f"got {v}")
class Suppress:
    def __enter__(self): return self
    def __exit__(self, exc_type, *a):
        return exc_type is ValueError
with Suppress():
    raise ValueError("suppressed")
print("after suppress")
