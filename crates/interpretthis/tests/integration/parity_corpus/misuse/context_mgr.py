class CM:
    def __enter__(self):
        print("enter")
        return self
    def __exit__(self, *a):
        print("exit", a[0].__name__ if a[0] else None)
        return False
with CM() as c:
    print("body")
try:
    with CM() as c:
        raise ValueError("boom")
except ValueError:
    print("caught")
