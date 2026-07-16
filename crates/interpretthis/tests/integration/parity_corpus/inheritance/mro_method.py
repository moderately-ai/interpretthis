# type.mro() returns the method resolution order as a list of type objects,
# always ending in `object`. Distinct from the deliberately-blocked `__mro__`
# attribute; the escape gate `__subclasses__` stays blocked.
class Base:
    pass


class Sub(Base):
    pass


class Other:
    pass


class Multi(Sub, Other):
    pass


class MyErr(Exception):
    pass


class SubValue(ValueError):
    pass


print([c.__name__ for c in Base.mro()])
print([c.__name__ for c in Sub.mro()])
print([c.__name__ for c in Multi.mro()])
print([c.__name__ for c in MyErr.mro()])
print([c.__name__ for c in SubValue.mro()])

# Reached through a temporary (`type(instance)`) and on builtin type objects.
print([c.__name__ for c in type(Sub()).mro()])
print([c.__name__ for c in int.mro()])
print([c.__name__ for c in bool.mro()])
print([c.__name__ for c in str.mro()])
print([c.__name__ for c in ValueError.mro()])
print([c.__name__ for c in object.mro()])

# Stable across calls.
print(Sub.mro() == Sub.mro())

# A user-defined `mro` method shadows the metaclass one.
class Shadow:
    def mro(self):
        return "shadowed"


print(Shadow().mro())
