# property (getter/setter/deleter), classmethod, staticmethod, descriptors.
class Temp:
    def __init__(self, c):
        self._c = c
    @property
    def celsius(self):
        return self._c
    @celsius.setter
    def celsius(self, v):
        if v < -273.15:
            raise ValueError("too cold")
        self._c = v
    @property
    def fahrenheit(self):
        return self._c * 9 / 5 + 32
    @classmethod
    def from_f(cls, f):
        return cls((f - 32) * 5 / 9)
    @staticmethod
    def is_freezing(c):
        return c <= 0

t = Temp(25)
print(t.celsius, t.fahrenheit)
t.celsius = 100
print(t.celsius, t.fahrenheit)
print(Temp.from_f(212).celsius, Temp.is_freezing(-5), Temp.is_freezing(10))
try:
    t.celsius = -300
except ValueError as e:
    print("err:", e)

# Custom descriptor
class Positive:
    def __set_name__(self, owner, name):
        self.name = "_" + name
    def __get__(self, obj, objtype=None):
        if obj is None:
            return self
        return getattr(obj, self.name, 0)
    def __set__(self, obj, value):
        if value < 0:
            raise ValueError("must be positive")
        setattr(obj, self.name, value)

class Account:
    balance = Positive()
    def __init__(self, b):
        self.balance = b

a = Account(100)
print(a.balance)
a.balance = 50
print(a.balance)
try:
    a.balance = -10
except ValueError as e:
    print("err:", e)

# classmethod inheritance and cls binding
class Base:
    @classmethod
    def create(cls):
        return cls.__name__

class Derived(Base):
    pass

print(Base.create(), Derived.create())

# super() in methods
class A:
    def greet(self):
        return "A"

class B(A):
    def greet(self):
        return "B+" + super().greet()

class C(B):
    def greet(self):
        return "C+" + super().greet()

print(C().greet())
