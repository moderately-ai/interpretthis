class GetAttribute:
    def __getattribute__(self, name):
        if name == "special":
            return "intercepted"
        return super().__getattribute__(name)
    def __init__(self):
        self.normal = "normal_value"
    def greet(self):
        return "hi"
ga = GetAttribute()
print(ga.special)
print(ga.normal)
print(ga.greet())
class Logged:
    def __init__(self):
        self._data = {"a": 1}
    def __getattribute__(self, name):
        if name.startswith("get_"):
            key = name[4:]
            return object.__getattribute__(self, "_data")[key]
        return object.__getattribute__(self, name)
lg = Logged()
print(lg.get_a)
print(lg._data)
class Chained:
    x = 10
    def __getattribute__(self, name):
        return super().__getattribute__(name)
c = Chained()
print(c.x)
try:
    print(c.missing)
except AttributeError:
    print("missing raised")
class Counter:
    def __init__(self):
        object.__setattr__(self, "n", 0)
    def __getattribute__(self, name):
        return super().__getattribute__(name)
    @property
    def doubled(self):
        return self.n * 2
cnt = Counter()
print(cnt.doubled)
