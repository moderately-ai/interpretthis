class Animal:
    kingdom = "Animalia"
    def __init__(self, name):
        self.name = name
    def sound(self):
        return "..."
    def __repr__(self):
        return f"{type(self).__name__}({self.name!r})"
class Dog(Animal):
    def sound(self):
        return "Woof"
class Puppy(Dog):
    def sound(self):
        return "Yip"
d = Dog("Rex")
print(d.sound())
print(d.kingdom)
print(repr(d))
print(isinstance(d, Animal))
print(Puppy("Max").sound())
print([type(x).__name__ for x in [Animal("a"), Dog("b"), Puppy("c")]])
class Mixin:
    def extra(self):
        return "mixed"
class Combined(Dog, Mixin):
    pass
c = Combined("Fido")
print(c.sound(), c.extra())
print(Animal.kingdom, Dog.kingdom)
