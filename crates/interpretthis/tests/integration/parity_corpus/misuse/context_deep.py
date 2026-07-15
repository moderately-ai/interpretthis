class Resource:
    def __init__(self, name):
        self.name = name
    def __enter__(self):
        return self.name
    def __exit__(self, *a):
        return False
with Resource("db") as r:
    print(r)
results = []
class Tracked:
    def __enter__(self):
        results.append("enter")
        return self
    def __exit__(self, exc_type, *a):
        results.append("exit" if exc_type is None else "error")
        return True
with Tracked():
    raise ValueError("boom")
print(results)
from contextlib import contextmanager
@contextmanager
def ctx():
    print("before")
    yield 42
    print("after")
with ctx() as v:
    print(v)
