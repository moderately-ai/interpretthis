class Base:
    registry = {}
    def __init_subclass__(cls, category=None, **kwargs):
        super().__init_subclass__(**kwargs)
        if category:
            Base.registry[category] = cls
class Dog(Base, category="animal"):
    pass
class Car(Base, category="vehicle"):
    pass
print(sorted(Base.registry.keys()))
print(Base.registry["animal"].__name__)
class Config(Base, category="settings"):
    pass
print(len(Base.registry))
