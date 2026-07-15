class Ctx:
    def __init__(self, name):
        self.name = name
    def __enter__(self):
        print(f"enter {self.name}")
        return self.name
    def __exit__(self, *args):
        print(f"exit {self.name}")
        return False
with Ctx("A") as a:
    print(f"body {a}")
with Ctx("B") as b, Ctx("C") as c:
    print(f"body {b} {c}")
results = []
with Ctx("D"):
    results.append(1)
    results.append(2)
print(results)
def use_ctx():
    with Ctx("E") as e:
        return e
print(use_ctx())
class Suppressing:
    def __enter__(self):
        return self
    def __exit__(self, exc_type, exc_val, tb):
        return exc_type is not None
with Suppressing():
    raise ValueError("gone")
print("survived")
