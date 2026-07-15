class Animal:
    def __init__(self, name):
        self.name = name
    def speak(self):
        return "..."
    def describe(self):
        return f"{self.name} says {self.speak()}"
class Dog(Animal):
    def __init__(self, name, breed):
        super().__init__(name)
        self.breed = breed
    def speak(self):
        return "Woof"
d = Dog("Rex", "Lab")
print(d.describe())
print(d.name, d.breed)
print(isinstance(d, Animal))
print(issubclass(Dog, Animal))
class A:
    def method(self):
        return "A"
class B(A):
    def method(self):
        return "B->" + super().method()
class C(B):
    def method(self):
        return "C->" + super().method()
print(C().method())
class Base:
    count = 0
    def __init__(self):
        Base.count += 1
Base()
Base()
print(Base.count)
