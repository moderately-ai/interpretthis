class Dynamic:
    def __getattr__(self, name):
        if name.startswith("get_"):
            return lambda: name[4:]
        raise AttributeError(name)
d = Dynamic()
print(d.get_hello())
print(d.get_world())
f = d.get_saved
print(f())
try:
    d.missing()
except AttributeError as e:
    print("AttributeError", str(e))
class WithCallableField:
    def __init__(self):
        self.cb = lambda x: x * 10
p = WithCallableField()
print(p.cb(5))
class Router:
    def __getattr__(self, name):
        return lambda *a: f"{name}:{a}"
r = Router()
print(r.foo(1, 2))
print(r.bar())
