class CM:
    def __init__(self, name):
        self.name = name
    def __enter__(self):
        print(f"enter {self.name}")
        return self.name
    def __exit__(self, *a):
        print(f"exit {self.name}")
        return False
with CM("a") as x:
    print(f"body {x}")
with CM("a") as a, CM("b") as b:
    print(f"nested {a} {b}")
try:
    with CM("c"):
        raise ValueError("boom")
except ValueError as e:
    print(f"caught {e}")
class Suppress:
    def __enter__(self): return self
    def __exit__(self, et, ev, tb): return et is ValueError
with Suppress():
    raise ValueError("swallowed")
print("survived")
