# Pins: del nested attribute paths (shared instance fields).
class Box:
    def __init__(self):
        self.x = 1

class Outer:
    def __init__(self):
        self.inner = Box()

o = Outer()
print(o.inner.x)
del o.inner.x
print(hasattr(o.inner, 'x'))

items = [Box(), Box()]
items[0].x = 9
del items[0].x
print(hasattr(items[0], 'x'), items[1].x)
