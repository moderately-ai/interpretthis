# Nested classes are registered as attributes of the enclosing class, are
# callable/instantiable through it, and carry a dotted `__qualname__`. The repr
# of any user class includes its `__main__` module prefix.
class Outer:
    class Inner:
        z = 9
        def hi(self): return "hi"
    class Middle:
        class Deep:
            tag = "deep"

print(Outer.Inner)
obj = Outer.Inner()
print(obj.z, obj.hi())
print(Outer.Inner.__name__, Outer.Inner.__qualname__)
print(Outer.Middle.Deep)
print(Outer.Middle.Deep.__qualname__, Outer.Middle.Deep.tag)
i = Outer.Inner()
print(isinstance(i, Outer.Inner))
print([Outer.Inner])

# Top-level class repr carries the module prefix; builtins do not.
class Top:
    pass
print(Top, Top.__name__, Top.__qualname__)
print(repr(Top), str(Top), f"{Top}")
print(int, str, list)

# A user exception class is also in __main__.
class MyError(Exception):
    pass
print(MyError)
