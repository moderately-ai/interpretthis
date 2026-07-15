import functools
@functools.singledispatch
def area(shape):
    return "unknown"
@area.register(int)
def _(shape):
    return f"int-area {shape}"
@area.register
def _(shape: str):
    return f"str-area {shape}"
print(area(5), area("box"), area(1.5))
class Animal: pass
class Dog(Animal): pass
class Cat(Animal): pass
@functools.singledispatch
def speak(a):
    return "..."
@speak.register
def _(a: Animal):
    return "generic animal"
@speak.register
def _(a: Dog):
    return "woof"
print(speak(Dog()), speak(Cat()), speak(Animal()), speak(42))
@functools.singledispatch
def sz(x):
    return -1
@sz.register(list)
def _(x):
    return len(x)
@sz.register(dict)
def _(x):
    return len(x) * 100
print(sz([1, 2, 3]), sz({"a": 1}), sz("no"))
@functools.singledispatch
def flag(x):
    return "default"
@flag.register(int)
def _(x):
    return f"int {x}"
print(flag(True), flag(5))
r = area.register(float)
def farea(shape):
    return f"float {shape}"
r(farea)
print(area(2.5))
print(area.__name__)
