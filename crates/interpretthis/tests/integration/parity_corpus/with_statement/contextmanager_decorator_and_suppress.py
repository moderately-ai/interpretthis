from contextlib import contextmanager, suppress, ExitStack, redirect_stdout
import io
@contextmanager
def managed():
    print("enter")
    yield "resource"
    print("exit")
with managed() as r:
    print("using", r)
@contextmanager
def with_cleanup():
    resources = []
    try:
        resources.append("a")
        yield resources
    finally:
        print("cleanup", resources)
with with_cleanup() as res:
    res.append("b")
    print("inside", res)
with suppress(ValueError):
    raise ValueError("ignored")
print("after suppress")
with suppress(KeyError, ValueError):
    raise KeyError("also ignored")
print("after multi suppress")
class Resource:
    def __init__(self, name):
        self.name = name
    def __enter__(self):
        print(f"acquire {self.name}")
        return self
    def __exit__(self, *args):
        print(f"release {self.name}")
        return False
with Resource("db") as r:
    print(f"using {r.name}")
with Resource("a") as a, Resource("b") as b:
    print("both")
f = io.StringIO()
with redirect_stdout(f):
    print("captured")
print("got:", f.getvalue().strip())
with ExitStack() as stack:
    stack.enter_context(Resource("x"))
    stack.enter_context(Resource("y"))
    print("in stack")
class Suppressing:
    def __enter__(self):
        return self
    def __exit__(self, exc_type, exc_val, tb):
        return exc_type is ValueError
with Suppressing():
    raise ValueError("swallowed")
print("survived")
count = 0
for i in range(3):
    with managed() as r:
        count += 1
print("count", count)
@contextmanager
def nested():
    with managed() as inner:
        yield inner
with nested() as n:
    print("nested value", n)
