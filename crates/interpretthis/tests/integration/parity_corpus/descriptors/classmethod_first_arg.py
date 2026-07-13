# @classmethod: the first arg is the class, not the instance. Pins
# instance_method_call's lookup_class_method early-return path.
class Counter:
    instances = 0

    @classmethod
    def reset(cls):
        # cls is the class. We don't expose direct class-attribute
        # writes from inside a classmethod yet, but the name access
        # works and the call shape pins the receiver.
        return cls.__name__

    @classmethod
    def factory(cls, label):
        # Calling cls() instantiates via the class's __init__.
        # Demonstrates that the bound first arg is genuinely the class.
        c = cls()
        c.label = label
        return c

    def __init__(self):
        self.label = ""

c = Counter()
print(Counter.reset())          # "Counter" — class name via cls
print(c.reset())                # same — call via instance, cls = class
made = Counter.factory("alpha")
print(made.label)               # "alpha"
