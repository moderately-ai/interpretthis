class Proxy:
    def __init__(self): self._d = {"x": 1}
    def __getattr__(self, name): return f"dynamic:{name}"
    def __setattr__(self, name, value):
        object.__setattr__(self, name, value)
    def __delattr__(self, name):
        print(f"del:{name}")
        object.__delattr__(self, name)
p = Proxy()
print(p.foo, p.bar)
print(p._d)
p.y = 5
print(p.y)
del p.y

class G:
    def __getattribute__(self, name):
        if name == "secret": return "hidden"
        return object.__getattribute__(self, name)
    val = 10
g = G()
print(g.secret, g.val)

print(getattr(p, "missing", "default"))
print(hasattr(p, "anything"))

class Dyn:
    def __getattr__(self, n):
        if n.startswith("get_"): return lambda: n[4:]
        raise AttributeError(n)
d = Dyn()
print(d.get_name())
try:
    d.other
except AttributeError as e:
    print("AE", e)
