class Meta(type):
    def __call__(cls, *a, **kw):
        print("meta call", a, kw)
        return super().__call__(*a, **kw)
class M(metaclass=Meta):
    def __init__(self, x):
        self.x = x
m = M(42)
print(m.x, type(m).__name__)
# Singleton pattern via metaclass
class Singleton(type):
    _instances = {}
    def __call__(cls, *a, **kw):
        if cls not in cls._instances:
            cls._instances[cls] = super().__call__(*a, **kw)
        return cls._instances[cls]
class DB(metaclass=Singleton):
    def __init__(self): self.id = 1
a = DB(); b = DB()
print(a is b)
# metaclass call with kwargs
class Meta2(type):
    def __call__(cls, *a, **kw):
        obj = super().__call__(*a, **kw)
        obj.created = True
        return obj
class Widget(metaclass=Meta2):
    def __init__(self, name): self.name = name
w = Widget("btn")
print(w.name, w.created)
# metaclass __call__ that overrides entirely
class Cache(type):
    def __call__(cls, key):
        return f"cached_{key}"
class Item(metaclass=Cache):
    pass
print(Item("abc"))
# normal class (no metaclass __call__) still works
class Plain:
    def __init__(self, v): self.v = v
print(Plain(7).v)
