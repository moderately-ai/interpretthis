class Descriptor:
    def __init__(self, name):
        self.private = "_" + name
    def __get__(self, obj, objtype=None):
        if obj is None:
            return self
        return getattr(obj, self.private, 0)
    def __set__(self, obj, value):
        setattr(obj, self.private, value * 2)
class MyClass:
    x = Descriptor("x")
    def __init__(self, val):
        self.x = val
m = MyClass(5)
print(m.x)
m.x = 10
print(m.x)
class Validated:
    def __set_name__(self, owner, name):
        self.private = f"_{name}"
    def __get__(self, obj, objtype=None):
        return getattr(obj, self.private, None)
    def __set__(self, obj, value):
        if value < 0:
            raise ValueError("negative")
        setattr(obj, self.private, value)
class Account:
    balance = Validated()
a = Account()
a.balance = 100
print(a.balance)
