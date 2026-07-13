# Pins: `map(Cls.classmethod, items)` — classmethod passed to map.
# Class methods receive the class implicitly when called normally; when
# stored as a value the binding must persist through indirection.
class Cls:
    @classmethod
    def named(cls, n):
        return f"{cls.__name__}-{n}"
print(list(map(Cls.named, [1, 2, 3])))
