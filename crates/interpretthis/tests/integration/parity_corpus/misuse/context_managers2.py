from contextlib import contextmanager, suppress
@contextmanager
def managed():
    print("enter")
    yield "resource"
    print("exit")
with managed() as r:
    print(f"using {r}")
with suppress(ValueError):
    raise ValueError("suppressed")
print("after suppress")
with suppress(KeyError, ValueError):
    raise KeyError("k")
print("after multi suppress")
@contextmanager
def with_error():
    print("setup")
    try:
        yield
    finally:
        print("teardown")
try:
    with with_error():
        raise RuntimeError("boom")
except RuntimeError:
    print("caught")
