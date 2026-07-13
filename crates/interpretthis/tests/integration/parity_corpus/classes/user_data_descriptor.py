# Pins: user descriptor protocol via __get__/__set__.
class Logged:
    def __init__(self):
        self.storage = {}

    def __get__(self, obj, owner=None):
        if obj is None:
            return self
        return self.storage.get(id(obj), "missing")

    def __set__(self, obj, value):
        self.storage[id(obj)] = value

class C:
    x = Logged()

c = C()
print(c.x)
c.x = "hello"
print(c.x)
