# Zero-arg super() reads the current method frame (defining class + self).
# Pins state.method_frame_stack + super_method_call: super().__init__()
# in Child runs Parent's __init__ on the same instance.
class Animal:
    def __init__(self, kind):
        self.kind = kind

    def describe(self):
        return f"a {self.kind}"

class Dog(Animal):
    def __init__(self, name):
        super().__init__("dog")
        self.name = name

    def describe(self):
        # super().describe() resumes from the next MRO slot — Animal.
        base = super().describe()
        return f"{self.name} is {base}"

d = Dog("Rex")
print(d.kind)               # set by Animal.__init__ via super
print(d.name)
print(d.describe())
