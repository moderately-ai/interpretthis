# Pins: len(obj) dispatches __len__, `x in obj` / `x not in obj`
# dispatch __contains__ on user-class instances.
class Bag:
    def __init__(self, items):
        self.items = items
    def __len__(self):
        return len(self.items)
    def __contains__(self, item):
        return item in self.items

b = Bag([1, 2, 3])
print(len(b))
print(2 in b)
print(99 in b)
print(99 not in b)
print(2 not in b)
