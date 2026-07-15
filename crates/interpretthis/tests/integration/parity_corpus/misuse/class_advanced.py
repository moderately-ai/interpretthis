class Base:
    def __init_subclass__(cls, **kwargs):
        super().__init_subclass__(**kwargs)
        cls.registered = True
class Derived(Base):
    pass
print(Derived.registered)
class Counter:
    _count = 0
    def __init__(self):
        type(self)._count += 1
    @classmethod
    def get_count(cls):
        return cls._count
Counter()
Counter()
print(Counter.get_count())
class Shape:
    @staticmethod
    def describe():
        return "a shape"
    @classmethod
    def create(cls):
        return cls()
print(Shape.describe())
class Named:
    def __init__(self, name):
        self.name = name
    def __repr__(self):
        return f"Named({self.name!r})"
    def __eq__(self, other):
        return isinstance(other, Named) and self.name == other.name
    def __hash__(self):
        return hash(self.name)
print(repr(Named("test")))
print(Named("a") == Named("a"))
print(len({Named("x"), Named("x"), Named("y")}))
