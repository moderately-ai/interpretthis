# Pins: user-class __getitem__/__setitem__/__delitem__ dispatch for
# the subscript surface (`obj[k]`, `obj[k] = v`, `del obj[k]`).
# Customer pattern: custom mapping types (CaseInsensitiveDict),
# struct-of-arrays accessors, lazy proxies.
class Box:
    def __init__(self):
        self.data = {}
    def __getitem__(self, k):
        return self.data[k]
    def __setitem__(self, k, v):
        self.data[k] = v
    def __delitem__(self, k):
        del self.data[k]
    def __repr__(self):
        return f"Box({self.data!r})"

b = Box()
b['x'] = 1
b['y'] = 2
print(b['x'])
print(b['y'])
print(b)
del b['x']
print(b)
