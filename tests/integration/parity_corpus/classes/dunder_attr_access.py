# Pins: __getattr__ fires on attribute miss; __setattr__ intercepts
# every set; __delattr__ intercepts every delete. The classes here
# avoid `self.__dict__` (which we don't model) by routing writes
# through `super().__setattr__` — a portable CPython idiom.
class Defaulter:
    """Fallback for missing attributes."""
    def __getattr__(self, name):
        return f"<missing:{name}>"

class Counter:
    """Counts how many sets and deletes happen via the slots."""
    def __init__(self):
        super().__setattr__("set_count", 0)
        super().__setattr__("del_count", 0)
    def __setattr__(self, name, value):
        super().__setattr__("set_count", self.set_count + 1)
        super().__setattr__(name, value)
    def __delattr__(self, name):
        super().__setattr__("del_count", self.del_count + 1)
        super().__delattr__(name)

d = Defaulter()
print(d.unknown)
print(d.anything)

c = Counter()
c.x = 1
c.y = 2
c.z = 3
print(c.x, c.y, c.z)
print(c.set_count)
del c.y
print(c.del_count)
