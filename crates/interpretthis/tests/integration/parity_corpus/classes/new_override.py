class Counter2:
    _instances = 0
    def __new__(cls):
        cls._instances += 1
        return super().__new__(cls)
Counter2(); Counter2()
print(Counter2._instances)
class Singleton:
    _inst = None
    def __new__(cls):
        if cls._inst is None:
            cls._inst = super().__new__(cls)
        return cls._inst
a = Singleton(); b = Singleton()
print(a is b)
class WithInit:
    def __new__(cls, v):
        obj = super().__new__(cls)
        obj.created = True
        return obj
    def __init__(self, v):
        self.v = v
w = WithInit(42)
print(w.v, w.created)
