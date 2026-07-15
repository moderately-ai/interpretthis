class Vec:
    def __init__(self, x, y):
        self.x, self.y = x, y
    def __add__(self, o):
        return Vec(self.x + o.x, self.y + o.y)
    def __eq__(self, o):
        return (self.x, self.y) == (o.x, o.y)
    def __repr__(self):
        return f"Vec({self.x}, {self.y})"
    def __len__(self):
        return 2
    def __getitem__(self, i):
        return (self.x, self.y)[i]
    def __iter__(self):
        return iter((self.x, self.y))
v = Vec(1, 2) + Vec(3, 4)
print(v)
print(v == Vec(4, 6))
print(len(v))
print(v[0], v[1])
print(list(v))
print(sum(v))
class Container:
    def __init__(self):
        self.data = [1, 2, 3]
    def __contains__(self, item):
        return item in self.data
    def __len__(self):
        return len(self.data)
c = Container()
print(2 in c, 5 in c)
print(len(c))
print(bool(c))
class Callable:
    def __call__(self, x):
        return x * 2
cc = Callable()
print(cc(21))
print(callable(cc))
