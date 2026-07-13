# isinstance() walks the MRO so Child instances are also Parent / object.
# Pins check_isinstance's class.mro walk.
class Animal:
    pass

class Dog(Animal):
    pass

class Puppy(Dog):
    pass

p = Puppy()
print(isinstance(p, Puppy))
print(isinstance(p, Dog))
print(isinstance(p, Animal))
print(isinstance(p, object))    # every class implicitly inherits object
print(isinstance(p, str))       # negative — Puppy is not str

# issubclass walks too.
print(issubclass(Puppy, Animal))
print(issubclass(Dog, Animal))
print(issubclass(Animal, Dog))  # False — Animal is not a subclass of Dog
print(issubclass(Puppy, object))
