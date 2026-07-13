# Pins: Parent.__init_subclass__ runs when a subclass is defined.
seen = []

class Parent:
    def __init_subclass__(cls, **kwargs):
        seen.append(cls.__name__)

class Child(Parent):
    pass

class Grand(Child):
    pass

print(seen)
