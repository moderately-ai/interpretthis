class Vec:
    def __init__(self, x, y):
        self.x = x
        self.y = y
    def __add__(self, o):
        return Vec(self.x + o.x, self.y + o.y)
    def __eq__(self, o):
        return self.x == o.x and self.y == o.y
    def __repr__(self):
        return f"Vec({self.x}, {self.y})"
    def __lt__(self, o):
        return (self.x**2 + self.y**2) < (o.x**2 + o.y**2)
    def __mul__(self, s):
        return Vec(self.x * s, self.y * s)
    def __len__(self):
        return 2
    def __getitem__(self, i):
        return (self.x, self.y)[i]
print(Vec(1, 2) + Vec(3, 4))
print(Vec(1, 2) == Vec(1, 2))
print(Vec(1, 1) < Vec(3, 3))
print(Vec(2, 3) * 2)
print(len(Vec(1, 2)))
print(Vec(5, 6)[0], Vec(5, 6)[1])
print(sorted([Vec(3, 3), Vec(1, 1), Vec(2, 2)]))
v = Vec(1, 2)
print(list(v))
