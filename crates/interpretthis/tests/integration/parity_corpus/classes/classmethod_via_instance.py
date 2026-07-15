# Calling a classmethod through an instance must not rebind the instance
# variable to the class: `s.cm()` returns the class name but `s` stays an
# instance, so later isinstance/attribute access still work.
class Base:
    x = "class-var"
    def __init__(self):
        self.y = "inst-var"
    @classmethod
    def cm(cls):
        return cls.__name__
    @staticmethod
    def sm():
        return "static"


class Sub(Base):
    def __init__(self):
        super().__init__()
        self.z = "sub-var"


s = Sub()
print(s.cm(), Sub.cm(), Base.sm())
# The instance survives the classmethod call.
print(isinstance(s, Base), isinstance(s, Sub))
print(s.y, s.z, s.x)
print(type(s).__name__)

# A classmethod that constructs via cls, called on an instance.
class Registry:
    items = []
    def __init__(self, name):
        self.name = name
    @classmethod
    def create(cls, name):
        obj = cls(name)
        return obj


r = Registry("first")
r2 = r.create("second")
print(r.name, r2.name, isinstance(r, Registry), isinstance(r2, Registry))
print(r.create("third").name, r.name)
