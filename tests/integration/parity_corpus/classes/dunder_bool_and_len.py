# Pins: user-class __bool__ wins truthiness; __len__ is the fallback
# when __bool__ is absent. Plain instance with neither is truthy.
class Bag:
    def __init__(self, items):
        self.items = items
    def __len__(self):
        return len(self.items)

class Flag:
    def __init__(self, on):
        self.on = on
    def __bool__(self):
        return self.on

empty = Bag([])
full = Bag([1, 2])
print(bool(empty))
print(bool(full))
print(not empty)
print(not full)
if full:
    print("full truthy")
if not empty:
    print("empty falsy")

on = Flag(True)
off = Flag(False)
print(bool(on))
print(bool(off))
print(on and "yes")
print(off or "fallback")
