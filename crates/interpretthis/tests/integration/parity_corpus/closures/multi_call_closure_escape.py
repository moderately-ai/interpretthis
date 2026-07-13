# Pins: when an inner function captures a mutable list from its
# enclosing scope, mutations through the closure are visible to the
# outer scope across repeated calls. The list's identity must survive
# every escape — Arc-share (D2) guarantees that the closure's captured
# reference points at the same backing storage as the outer name.
def make_appender():
    items = []
    def append(x):
        items.append(x)
        return items
    return append, items

append, items = make_appender()
append(1)
append(2)
append(3)
print(items)
