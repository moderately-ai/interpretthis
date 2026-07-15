class C:
    count = 0
    def __init__(self, x):
        self.x = x
        C.count += 1
    @property
    def doubled(self):
        return self.x * 2
    @doubled.setter
    def doubled(self, v):
        self.x = v // 2
    @staticmethod
    def sm():
        return "static"
    @classmethod
    def cm(cls):
        return cls.count
    def __repr__(self):
        return f"C({self.x})"
a = C(5)
b = C(10)
print(a.doubled)
a.doubled = 20
print(a.x)
print(C.sm())
print(C.cm())
print(repr(a), a)
print(C.count)
class D(C):
    def __init__(self, x, y):
        super().__init__(x)
        self.y = y
d = D(1, 2)
print(d.x, d.y, d.doubled)
print(isinstance(d, C), issubclass(D, C))
