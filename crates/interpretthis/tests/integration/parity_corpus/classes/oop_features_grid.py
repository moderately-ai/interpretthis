class Animal:
    def __init__(self, name):
        self.name = name
    def speak(self):
        return "..."
    def __repr__(self):
        return f"Animal({self.name!r})"
class Dog(Animal):
    def speak(self):
        return "Woof"
    def fetch(self):
        return f"{self.name} fetches"
d = Dog("Rex")
print(d.speak(), d.fetch(), repr(d))
print(isinstance(d, Animal), isinstance(d, Dog), issubclass(Dog, Animal))
print(hasattr(d, "name"), getattr(d, "name"), getattr(d, "x", "def"))
class Counter:
    count = 0
    def __init__(self):
        Counter.count += 1
Counter(); Counter(); Counter()
print(Counter.count)
class Vec:
    def __init__(self, x): self.x = x
    def __add__(self, o): return Vec(self.x + o.x)
    def __eq__(self, o): return self.x == o.x
    def __repr__(self): return f"Vec({self.x})"
print(Vec(1) + Vec(2), Vec(3) == Vec(3))
