# __bool__, __int__, __float__, __index__, __len__ coercion dunders.
class Truthy:
    def __init__(self, b):
        self.b = b
    def __bool__(self):
        return self.b

print(bool(Truthy(True)), bool(Truthy(False)))
print("yes" if Truthy(True) else "no", "yes" if Truthy(False) else "no")

class Num:
    def __init__(self, n):
        self.n = n
    def __int__(self):
        return int(self.n)
    def __float__(self):
        return float(self.n)
    def __index__(self):
        return int(self.n)

print(int(Num(3.7)), float(Num(5)))
print([1, 2, 3, 4, 5][Num(2)], list(range(10))[Num(3):Num(7)])
print(bin(Num(10)), hex(Num(255)), "abc"[Num(1)])

class Container:
    def __init__(self, items):
        self.items = items
    def __len__(self):
        return len(self.items)
    def __bool__(self):
        return len(self.items) > 2

print(len(Container([1, 2, 3])), bool(Container([1])), bool(Container([1, 2, 3])))

# __enter__/__exit__ context manager
class Ctx:
    def __init__(self, name):
        self.name = name
    def __enter__(self):
        print(f"enter {self.name}")
        return self
    def __exit__(self, *args):
        print(f"exit {self.name}")
        return False

with Ctx("a") as c:
    print(f"body {c.name}")

# context manager suppressing exception
class Suppress:
    def __enter__(self):
        return self
    def __exit__(self, exc_type, exc_val, tb):
        return exc_type is ValueError

with Suppress():
    raise ValueError("suppressed")
print("after suppress")

# chr / range / other int-accepting builtins also coerce via __index__.
print(chr(Num(65)), chr(Num(97)))
print(list(range(Num(2), Num(5))), "hello"[Num(0):Num(3)])
print(oct(Num(64)), hex(Num(16)))
