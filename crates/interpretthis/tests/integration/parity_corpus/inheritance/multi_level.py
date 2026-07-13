# Multi-level inheritance via C3 MRO. Pins build_mro + lookup_method_in_mro:
# Child inherits both attrs and methods from Parent and Grandparent without
# redefining them.
class A:
    def greet(self):
        return "hello from A"

    def species(self):
        return "human"

class B(A):
    def greet(self):
        return "hello from B"

class C(B):
    pass

a = A()
b = B()
c = C()
print(a.greet())
print(b.greet())
print(c.greet())            # inherited from B per MRO
print(c.species())          # inherited from A via two-level walk
