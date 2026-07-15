class Counter:
    def __init__(self):
        self.n = 0
    def __call__(self, x):
        self.n += x
        return self.n
    def __contains__(self, x):
        return x <= self.n
c = Counter()
print(c(5))
print(c(3))
print(5 in c)
print(100 in c)
class Temp:
    def __init__(self, celsius):
        self._c = celsius
    @property
    def fahrenheit(self):
        return self._c * 9/5 + 32
    @fahrenheit.setter
    def fahrenheit(self, value):
        self._c = (value - 32) * 5/9
t = Temp(100)
print(t.fahrenheit)
t.fahrenheit = 32
print(t._c)
class Math:
    @staticmethod
    def add(a, b):
        return a + b
    @classmethod
    def create(cls):
        return cls()
print(Math.add(2, 3))
