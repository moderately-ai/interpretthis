class Box:
    def __init__(self, v):
        self.v = v
    def __radd__(self, o):
        return o + self.v
    def __iadd__(self, o):
        self.v += o
        return self
    def __neg__(self):
        return Box(-self.v)
    def __abs__(self):
        return abs(self.v)
    def __bool__(self):
        return self.v != 0
    def __hash__(self):
        return hash(self.v)
    def __eq__(self, o):
        return isinstance(o, Box) and self.v == o.v
    def __int__(self):
        return int(self.v)
print(5 + Box(3))
b = Box(10)
b += 5
print(b.v)
print((-Box(7)).v)
print(abs(Box(-4)))
print(bool(Box(0)), bool(Box(1)))
print(int(Box(3.7)))
d = {Box(1): "one"}
print(d[Box(1)])
