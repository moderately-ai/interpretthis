class Temp:
    def __init__(self): self._c = 0
    @property
    def celsius(self):
        "the temperature in celsius"
        return self._c
    @celsius.setter
    def celsius(self, v): self._c = v
    @property
    def fahrenheit(self): return self._c * 9/5 + 32
t = Temp()
t.celsius = 25
print(t.celsius, t.fahrenheit)
print(type(Temp.celsius).__name__)
print(type(Temp.fahrenheit).__name__)
print(Temp.celsius.fget(t))
print(Temp.celsius.fset is not None, Temp.celsius.fdel is None)
print(Temp.fahrenheit.fset is None)
print(Temp.celsius.__doc__)
print(Temp.fahrenheit.__doc__)
print(Temp.celsius.fget.__name__)
class Sub(Temp): pass
print(type(Sub.celsius).__name__)
print(Sub.celsius.fget(t))
print(isinstance(Temp.celsius, property))
