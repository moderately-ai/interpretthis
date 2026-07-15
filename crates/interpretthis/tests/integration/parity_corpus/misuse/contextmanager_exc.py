from contextlib import contextmanager
@contextmanager
def suppressing():
    try:
        yield "resource"
    except ValueError:
        print("suppressed in cm")
with suppressing() as r:
    print(r)
    raise ValueError("boom")
print("after suppress")
@contextmanager
def cleanup():
    print("acquire")
    try:
        yield
    finally:
        print("release")
try:
    with cleanup():
        print("body")
        raise RuntimeError("fail")
except RuntimeError:
    print("caught outside")
@contextmanager
def simple():
    yield 1
    yield 2
try:
    with simple():
        pass
except RuntimeError as e:
    print("didn't stop")
