class Proxy:
    def __init__(self):
        self._data = {"x": 1, "y": 2}
    def __getattr__(self, name):
        if name in self._data:
            return self._data[name]
        raise AttributeError(name)
p = Proxy()
print(p.x, p.y)
try:
    p.z
except AttributeError:
    print("no z")
class Dynamic:
    def __getattr__(self, name):
        return f"dynamic_{name}"
d = Dynamic()
print(d.foo, d.bar)
class Recorder:
    def __init__(self):
        object.__setattr__(self, "log", [])
    def __setattr__(self, name, value):
        self.log.append((name, value))
        object.__setattr__(self, name, value)
r = Recorder()
r.a = 1
r.b = 2
print(r.log)
print(r.a, r.b)
class WithDefault:
    def __init__(self):
        self.x = 10
    def __getattr__(self, name):
        return 0
w = WithDefault()
print(w.x, w.missing)
