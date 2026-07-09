# Pins: multiple inheritance via C3 linearization — C(A, B) inherits
# from A first, so C().name() returns "A" not "B".
class A:
    def name(self): return "A"

class B:
    def name(self): return "B"

class C(A, B):
    pass

print(C().name())
