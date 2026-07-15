class Temperature:
    def __init__(self, celsius):
        self.celsius = celsius
    def __str__(self):
        return f"{self.celsius}°C"
    def __repr__(self):
        return f"Temperature({self.celsius})"
    def __format__(self, spec):
        if spec == "f":
            return f"{self.celsius * 9/5 + 32}°F"
        return str(self)
t = Temperature(25)
print(str(t))
print(repr(t))
print(f"{t}")
print(f"{t:f}")
print("{}".format(t))
print("{!r}".format(t))
print([t])
class Boolish:
    def __init__(self, val):
        self.val = val
    def __bool__(self):
        return bool(self.val)
print(bool(Boolish(0)), bool(Boolish(1)))
print("yes" if Boolish(5) else "no")
if Boolish(0):
    print("truthy")
else:
    print("falsy")
class Hashable:
    def __init__(self, k):
        self.k = k
    def __hash__(self):
        return hash(self.k)
    def __eq__(self, o):
        return self.k == o.k
print(len({Hashable(1), Hashable(1), Hashable(2)}))
print(Hashable("x") in {Hashable("x")})
