class Meta(type):
    def __new__(mcs, name, bases, ns):
        ns["created_by_meta"] = True
        return super().__new__(mcs, name, bases, ns)
class WithMeta(metaclass=Meta):
    pass
print(WithMeta.created_by_meta)

class Meta(type):
    def __new__(mcs, name, bases, ns):
        ns["created_by_meta"] = True
        ns["class_id"] = name.lower()
        return super().__new__(mcs, name, bases, ns)
class A(metaclass=Meta):
    x = 10
class B(metaclass=Meta):
    y = 20
print(A.created_by_meta, A.class_id, A.x)
print(B.created_by_meta, B.class_id, B.y)
class RegistryMeta(type):
    registry = {}
    def __new__(mcs, name, bases, ns):
        cls = super().__new__(mcs, name, bases, ns)
        if name != "Base":
            RegistryMeta.registry[name] = cls
        return cls
class Base(metaclass=RegistryMeta):
    pass
class Plugin1(Base):
    pass
class Plugin2(Base):
    pass
print(sorted(RegistryMeta.registry.keys()))
class InitMeta(type):
    def __init__(cls, name, bases, ns):
        super().__init__(name, bases, ns)
        cls.initialized = True
class Uses(metaclass=InitMeta):
    pass
print(Uses.initialized)
class UpperMeta(type):
    def __new__(mcs, name, bases, ns):
        upper = {k.upper() if not k.startswith("__") else k: v for k, v in ns.items()}
        return super().__new__(mcs, name, bases, upper)
class Config(metaclass=UpperMeta):
    debug = True
    verbose = False
print(Config.DEBUG, Config.VERBOSE)
class MethodMeta(type):
    def __new__(mcs, name, bases, ns):
        ns["greet"] = lambda self: f"hi from {name}"
        return super().__new__(mcs, name, bases, ns)
class Greeter(metaclass=MethodMeta):
    pass
print(Greeter().greet())
