# Pins: metaclass=__new__ can inject class attrs via type(name, bases, ns).
class Meta:
    def __new__(cls, name, bases, ns):
        ns["tag"] = 42
        return type(name, bases, ns)

class C(metaclass=Meta):
    pass

print(C.tag)
